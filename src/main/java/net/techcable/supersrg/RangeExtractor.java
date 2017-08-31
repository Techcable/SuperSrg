package net.techcable.supersrg;

import lombok.*;
import spoon.Launcher;
import spoon.SpoonModelBuilder;
import spoon.processing.ProcessingManager;
import spoon.reflect.CtModel;
import spoon.support.QueueProcessingManager;

import java.io.File;
import java.io.IOException;
import java.util.Collections;
import java.util.List;
import java.util.Objects;
import java.util.stream.Collectors;
import javax.annotation.Nullable;

import com.google.common.collect.ImmutableList;
import com.google.common.collect.ImmutableMap;

import io.netty.buffer.ByteBuf;
import io.netty.buffer.ByteBufOutputStream;
import io.netty.buffer.PooledByteBufAllocator;

import net.sourceforge.argparse4j.ArgumentParsers;
import net.sourceforge.argparse4j.impl.action.AppendArgumentAction;
import net.sourceforge.argparse4j.impl.action.StoreTrueArgumentAction;
import net.sourceforge.argparse4j.impl.type.FileArgumentType;
import net.sourceforge.argparse4j.inf.ArgumentParser;
import net.sourceforge.argparse4j.inf.Namespace;
import net.techcable.supersrg.cmd.CmdUtils;
import net.techcable.supersrg.source.MemberReferenceExtractor;
import net.techcable.supersrg.source.ProgressPrintingProcessor;
import net.techcable.supersrg.source.RangeMap;
import net.techcable.supersrg.source.RangeMapBuilder;
import net.techcable.supersrg.source.SpoonUtils;
import net.techcable.supersrg.utils.CompressionFormat;
import net.techcable.supersrg.utils.FastSerializationModelStreamer;
import net.techcable.supersrg.utils.FileHashWorker;
import net.techcable.supersrg.utils.SerializationUtils;

import static com.google.common.base.Preconditions.*;

public class RangeExtractor {
    private final File sourceDir;
    private final ImmutableList<File> classpath;
    public RangeExtractor(File sourceDir, List<File> classpath) {
        this.sourceDir = Objects.requireNonNull(sourceDir, "Null sourceFile");
        this.classpath = ImmutableList.copyOf(classpath);
    }
    private ImmutableMap<File, byte[]> currentHashes;
    @Setter
    private File cacheLocation = null;
    @Setter
    private boolean forceRebuild = false;

    public void computeHashes() {
        this.currentHashes = FileHashWorker.hashFiles("SHA-256", sourceDir);
    }
    private ImmutableMap<File, byte[]> getCurrentHashes() {
        ImmutableMap<File, byte[]> result = this.currentHashes;
        return result != null ? result : ImmutableMap.of();
    }

    public RangeMap extract(@Nullable RangeMap existingRangeMap) throws IOException {
        ImmutableMap<File, byte[]> currentHashes = this.getCurrentHashes();
        SpoonModelBuilder modelBuilder = new Launcher().createCompiler();
        modelBuilder.addInputSource(this.sourceDir);
        modelBuilder.setSourceClasspath(this.classpath.stream().map(File::getAbsolutePath).toArray(String[]::new));
        if (cacheLocation != null) {
            if (!cacheLocation.exists()) cacheLocation.mkdirs();
            modelBuilder.setBinaryOutputDirectory(cacheLocation);
            modelBuilder.setBuildOnlyOutdatedFiles(true);
        }
        if (existingRangeMap != null && !currentHashes.isEmpty()) {
            modelBuilder.addCompilationUnitFilter(path -> {
                File file = new File(path);
                checkState(CmdUtils.relativePath(file, sourceDir).equals(file), "File isn't relative: %", file);
                byte[] expectedHash = currentHashes.get(new File(path).getAbsoluteFile());
                if (expectedHash != null && existingRangeMap.hasFileHash(path, expectedHash)) {
                    return false;
                } else {
                    System.out.println("Recomputing ranges for " + path);
                    return true;
                }
            });
        }
        File cacheFile = cacheLocation != null ? new File(cacheLocation, "model.dat.lz4") : null;
        CtModel model;
        if (cacheFile != null && cacheFile.exists() && !forceRebuild) {
            System.out.println("Loading cached Spoon AST");
            System.out.println("WARN: No cache invalidation logic is currently implemented");
            ByteBuf compressed = PooledByteBufAllocator.DEFAULT.directBuffer();
            ByteBuf decompressed = PooledByteBufAllocator.DEFAULT.directBuffer();
            try {
                try {
                    SerializationUtils.loadFromFile(compressed, cacheFile);
                    System.out.println("Decompressing model");
                    CompressionFormat.LZ4_BLOCK.decompress(compressed, decompressed);
                } finally {
                    compressed.release();
                }
                System.out.println("Deserializing model");
                model = new FastSerializationModelStreamer().load(decompressed).getModel();
                // Make sure to set the proper source classpath so we can infer the CompilationUnits
                SpoonUtils.setSourceRoots(ImmutableList.of(this.sourceDir));
                // Make sure to set spoon's source classpath so we can load classes
                model.getRootPackage().getFactory().getEnvironment().setSourceClasspath(Objects.requireNonNull(modelBuilder.getSourceClasspath()));
            } finally {
                decompressed.release();
            }
        } else {
            System.out.println("Compiling Spoon AST");
            modelBuilder.build();
            model = modelBuilder.getFactory().getModel();
            if (cacheFile != null) {
                System.out.println("Serializing model");
                ByteBuf buffer = PooledByteBufAllocator.DEFAULT.directBuffer();
                ByteBuf compressed = PooledByteBufAllocator.DEFAULT.directBuffer();
                try {
                    try {
                        new FastSerializationModelStreamer().save(modelBuilder.getFactory(), new ByteBufOutputStream(buffer));
                        System.out.println("Compressing model");
                        CompressionFormat.LZ4_BLOCK.compress(buffer, compressed);
                    } finally {
                        buffer.release();
                    }
                    System.out.println("Saving model");
                    SerializationUtils.writeToFile(compressed, cacheFile);
                } finally {
                    if (compressed != null) compressed.release();
                }
            }
        }
        System.out.println("Recomputing range maps");
        RangeMapBuilder rangeMapBuilder = new RangeMapBuilder();
        model.processWith(new MemberReferenceExtractor(rangeMapBuilder, new ProgressPrintingProcessor(model)));
        System.out.println(); // End the progress line
        RangeMap result = existingRangeMap != null ? existingRangeMap : RangeMap.empty();
        return result.update(rangeMapBuilder.build());
    }

    @SuppressWarnings("UseOfSystemOutOrSystemErr")
    public static void main(String[] args) {
        CmdUtils.catchExceptions(() -> {
            ArgumentParser parser = ArgumentParsers.newArgumentParser("RangeExtractor")
                    .defaultHelp(true)
                    .description("Extracts a rangemap from source files to use with RangeApplier");
            parser.addArgument("-cp", "--classpath")
                    .help("Specify the classpath to use for analyzing the source files")
                    .action(new AppendArgumentAction());
            parser.addArgument("--cache")
                    .help("A directory to cache the Spoon AST model")
                    .type(new FileArgumentType());
            parser.addArgument("--rebuild")
                    .help("Rebuild the spoon AST model even if a cached version exists")
                    .action(new StoreTrueArgumentAction());
            parser.addArgument("sourceDir")
                    .required(true)
                    .type(new FileArgumentType().verifyExists().verifyIsDirectory())
                    .help("Specify the source files to extract ranges from");
            parser.addArgument("rangeMap")
                    .required(true)
                    .type(new FileArgumentType())
                    .help("The rangeMap file to output, reusing cached info if already present.");
            Namespace namespace = parser.parseArgsOrFail(args);
            List<File> classpath = namespace.getList("classpath") == null ? Collections.emptyList() :
                    namespace.getList("classpath").stream()
                            .map(String.class::cast)
                            .flatMap((s) -> CmdUtils.splitFiles(s).stream())
                            .collect(Collectors.toList());
            File sourceDir = namespace.get("sourceDir");
            File rangeMap = namespace.get("rangeMap");
            RangeMap existing = null;
            if (rangeMap.exists()) {
                System.out.println("Parsing existing range map");
                existing = RangeMap.load(rangeMap);
            }
            RangeExtractor extractor = new RangeExtractor(sourceDir, classpath);
            if (namespace.get("cache") != null) {
                extractor.cacheLocation = namespace.get("cache");
                extractor.forceRebuild = namespace.getBoolean("rebuild");
            }
            System.out.println("Computing sha256 of files, to check cache");
            extractor.computeHashes();

            RangeMap result = extractor.extract(existing);
            System.out.println("Saving range map");
            result.save(rangeMap);
        });
    }
}
