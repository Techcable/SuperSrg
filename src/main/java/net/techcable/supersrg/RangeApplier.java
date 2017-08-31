package net.techcable.supersrg;

import lombok.*;

import java.io.BufferedInputStream;
import java.io.BufferedOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.io.PrintWriter;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;
import java.util.Queue;
import java.util.concurrent.ConcurrentLinkedQueue;

import com.google.common.collect.ImmutableList;

import net.sourceforge.argparse4j.ArgumentParsers;
import net.sourceforge.argparse4j.impl.type.FileArgumentType;
import net.sourceforge.argparse4j.inf.ArgumentParser;
import net.sourceforge.argparse4j.inf.ArgumentParserException;
import net.sourceforge.argparse4j.inf.Namespace;
import net.techcable.srglib.format.MappingsFormat;
import net.techcable.srglib.mappings.ImmutableMappings;
import net.techcable.supersrg.binary.BinaryMappingsDecoder;
import net.techcable.supersrg.cmd.CmdUtils;
import net.techcable.supersrg.cmd.CommandException;
import net.techcable.supersrg.source.MemberReference;
import net.techcable.supersrg.source.RangeMap;
import net.techcable.supersrg.source.StreamRangeApplier;
import net.techcable.supersrg.utils.FastMappings;

public class RangeApplier {
    private final File inputDir, outputDir;
    private final FastMappings mappings;
    private final RangeMap rangeMap;
    public RangeApplier(File inputDir, File outputDir, FastMappings mappings, RangeMap rangeMap) {
        this.inputDir = Objects.requireNonNull(inputDir);
        this.outputDir = Objects.requireNonNull(outputDir);
        this.mappings = Objects.requireNonNull(mappings);
        this.rangeMap = rangeMap;
    }

    public void parallelApply() throws IOException {
        this.parallelApply(Runtime.getRuntime().availableProcessors());
    }
    public void parallelApply(int numThreads) throws IOException {
        Queue<File> files = new ConcurrentLinkedQueue<>(CmdUtils.recursivelyListFiles(this.inputDir));
        Object lock = new Object();
        List<Thread> threads = new ArrayList<>(numThreads);
        for (int i = 0; i < numThreads; i++) {
            threads.add(new Thread(new ParallelApplyTask(files, lock)));
        }
        threads.forEach(Thread::start);
        for (Thread thread : threads) {
            while (true) {
                try {
                    thread.join();
                    break;
                } catch (InterruptedException ignored) {}
            }
        }
    }

    @RequiredArgsConstructor
    private class ParallelApplyTask implements Runnable {
        private final Queue<File> fileQueue;
        private final Object lock;
        @Override
        public void run() {
            File file;
            while ((file = fileQueue.poll()) != null) {
                String relativePath = CmdUtils.relativePath(file, inputDir);
                try {
                    RangeApplier.this.applyFile(relativePath);
                } catch (IOException | IllegalStateException e) {
                    // NOTE: Lock ensures only one thread actually exits
                    synchronized (lock) {
                        System.err.println("Error applying file: " + e);
                        e.printStackTrace();
                        System.exit(1);
                    }
                }
            }
        }
    }

    public void applyFile(String relativePath) throws IOException {
        ImmutableList<MemberReference> references = rangeMap.allSortedReferences(relativePath);
        if (references == null) return;
        File outputFile = new File(outputDir, relativePath);
        outputFile.getParentFile().mkdirs();
        File inputFile = new File(inputDir, relativePath);
        try (
                InputStream in = new BufferedInputStream(new FileInputStream(inputFile));
                OutputStream out = new BufferedOutputStream(new FileOutputStream(outputFile))
        ) {
            StreamRangeApplier applier = new StreamRangeApplier(in, out, references);
            applier.apply(this.mappings);
        }
    }

    public static void main(String[] rawArgs) {
        CmdUtils.catchExceptions(() -> {
            ArgumentParser parser = ArgumentParsers.newArgumentParser("RangeApplier")
                    .defaultHelp(true)
                    .description("Applies mappings to source files using rangemaps generated with RangeExtractor");
            parser.addArgument("originalSources")
                    .required(true)
                    .type(new FileArgumentType().verifyIsDirectory())
                    .help("The original sources to apply the mappings to");
            parser.addArgument("outputDir")
                    .required(true)
                    .type(new FileArgumentType())
                    .help("The output directory to place the remapped soures");
            parser.addArgument("rangeMap")
                    .required(true)
                    .type(new FileArgumentType().verifyIsFile())
                    .help("The RangeMap to use to remap the sources");
            parser.addArgument("mappingsFile")
                    .required(true)
                    .type(new FileArgumentType().verifyIsFile())
                    .help("The mappings file to remap the sources with");
            final Namespace args;
            try {
                args = parser.parseArgs(rawArgs);
            } catch (ArgumentParserException e) {
                System.err.println("Invalid arguments: " + e.getMessage());
                e.getParser().printHelp(new PrintWriter(System.err, true));
                System.exit(1);
                throw new AssertionError();
            }
            File originalSources = args.get("originalSources");
            File outputDir = args.get("outputDir");
            File rangeMapFile = args.get("rangeMap");
            File mappingsFile = args.get("mappingsFile");
            System.out.println("Reading mappings from " + mappingsFile.getName());
            FastMappings mappings = FastMappings.fromFile(mappingsFile);
            System.out.println("Reading RangeMap from " + rangeMapFile.getName());
            RangeMap rangeMap = RangeMap.load(rangeMapFile);
            RangeApplier applier = new RangeApplier(originalSources, outputDir, mappings, rangeMap);
            System.out.println("Remapping files....");
            applier.parallelApply();
        });
    }
}
