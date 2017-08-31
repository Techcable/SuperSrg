package net.techcable.supersrg;

import lombok.*;

import java.io.BufferedOutputStream;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.util.ArrayList;
import java.util.Enumeration;
import java.util.List;
import java.util.Map;
import java.util.NoSuchElementException;
import java.util.Objects;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.jar.JarEntry;
import java.util.jar.JarFile;
import java.util.zip.ZipEntry;
import java.util.zip.ZipOutputStream;

import com.google.common.base.Verify;
import com.google.common.collect.Maps;
import com.google.common.io.Files;

import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;

import net.sourceforge.argparse4j.ArgumentParsers;
import net.sourceforge.argparse4j.impl.type.FileArgumentType;
import net.sourceforge.argparse4j.inf.ArgumentParser;
import net.sourceforge.argparse4j.inf.Namespace;
import net.techcable.supersrg.classfile.ConstantPoolDecodeException;
import net.techcable.supersrg.classfile.ConstantPoolDecoder;
import net.techcable.supersrg.classfile.ConstantPoolRemapper;
import net.techcable.supersrg.cmd.CmdUtils;
import net.techcable.supersrg.cmd.CommandException;
import net.techcable.supersrg.utils.FastMappings;

public class JarRemapper {
    private final FastMappings mappings;

    public JarRemapper(FastMappings mappings) {
        this.mappings = Objects.requireNonNull(mappings);
    }

    public void parallelRemapJar(JarFile inputFile, File outputFile, int numWorkers) {
        BlockingQueue<Map.Entry<String, ByteBuf>> outputQueue = new ArrayBlockingQueue<>(256);
        AtomicBoolean done = new AtomicBoolean(false);
        Thread jarOutputThread = new Thread(new JarOutputWorker(outputFile, outputQueue, done), "Jar output worker");
        int numRemapWorkers = Math.max(1, numWorkers - 1);
        List<Thread> remapThreads = new ArrayList<>(numRemapWorkers);
        Enumeration<JarEntry> entryIterator = inputFile.entries();
        for (int i = 0; i < numRemapWorkers; i++) {
            remapThreads.add(new Thread(new JarRemapWorker(
                    outputQueue,
                    inputFile,
                    entryIterator,
                    mappings
            ),"Jar remap worker #" + i));
        }
        remapThreads.forEach(Thread::start);
        jarOutputThread.start();
        for (Thread remapThread : remapThreads) {
            while (true) {
                try {
                    remapThread.join();
                    break;
                } catch (InterruptedException ignored) {}
            }
        }
        done.set(true);
        jarOutputThread.interrupt();
        while (true) {
            try {
                jarOutputThread.join();
                break;
            } catch (InterruptedException ignored) {}
        }
    }
    @RequiredArgsConstructor
    private static class JarRemapWorker implements Runnable {
        private final BlockingQueue<Map.Entry<String, ByteBuf>> outputQueue;
        private final JarFile inputFile;
        private final Enumeration<JarEntry> entryIterator;
        private final FastMappings mappings;
        @Override
        public void run() {
            ByteBuf inputBuffer = Unpooled.buffer();
            while (entryIterator.hasMoreElements()) {
                JarEntry entry;
                try {
                    entry = entryIterator.nextElement();
                } catch (NoSuchElementException e) {
                    // We must have lost a race
                    break;
                }
                String entryName = entry.getName();
                try {
                    inputBuffer.setIndex(0, 0);
                    try (InputStream in = inputFile.getInputStream(entry)) {
                        while (true) {
                            int numRead = inputBuffer.writeBytes(in, 4096);
                            if (numRead < 0) break;
                        }
                    }
                    if (entryName.endsWith(".class")) {
                        String originalClass = entryName.substring(0, entryName.length() - ".class".length());
                        FastMappings.ClassMappings classMappings = mappings.getClassMappings(originalClass);
                        final String remappedClass;
                        if (classMappings != null && classMappings.getRemappedName() != null) {
                            remappedClass = classMappings.getRemappedName();
                        } else {
                            remappedClass = originalClass;
                        }
                        ConstantPoolDecoder decoder = ConstantPoolDecoder.decode(inputBuffer);
                        int remainingData = inputBuffer.writerIndex() - decoder.getEnd();
                        Verify.verify(remainingData > 0);
                        ByteBuf outputBuffer = Unpooled.buffer(((int) (decoder.byteSize() * 1.5)) + remainingData);
                        new ConstantPoolRemapper(mappings, decoder, outputBuffer).remap(outputBuffer);
                        outputBuffer.writeBytes(inputBuffer, decoder.getEnd(), remainingData);
                        while (true) {
                            try {
                                outputQueue.put(Maps.immutableEntry(
                                        remappedClass + ".class",
                                        outputBuffer
                                ));
                                break;
                            } catch (InterruptedException ignored) {
                            }
                        }
                    } else {
                        while (true) {
                            try {
                                outputQueue.put(Maps.immutableEntry(entryName, Unpooled.copiedBuffer(inputBuffer)));
                                break;
                            } catch (InterruptedException ignored) {
                            }
                        }
                    }
                } catch (IOException | ConstantPoolDecodeException | IndexOutOfBoundsException e) {
                    System.err.println("Error remapping " + entryName);
                    e.printStackTrace();
                    System.exit(1);
                }
            }
        }
    }
    @RequiredArgsConstructor
    private static class JarOutputWorker implements Runnable {
        private final File outputFile;
        private final BlockingQueue<Map.Entry<String, ByteBuf>> outputQueue;
        private final AtomicBoolean done;
        @Override
        @SneakyThrows(IOException.class)
        public void run() {
            try (ZipOutputStream outputStream = new ZipOutputStream(new BufferedOutputStream(new FileOutputStream(outputFile)))) {
                while (true) {
                    Map.Entry<String, ByteBuf> entry;
                    try {
                        entry = outputQueue.take();
                    } catch (InterruptedException ignored) {
                        if (done.get()) break;
                        continue;
                    }
                    String name = entry.getKey();
                    ByteBuf buffer = entry.getValue();
                    try {
                        outputStream.putNextEntry(new ZipEntry(name));
                        buffer.getBytes(0, outputStream, buffer.readableBytes());
                    } finally {
                        buffer.release();
                    }
                }
            }
        }
    }

    public static void main(String[] args) {
        CmdUtils.catchExceptions(() -> {
            ArgumentParser parser = ArgumentParsers.newArgumentParser("JarRemapper")
                    .defaultHelp(true)
                    .description("Remap class files or jar files");
            parser.addArgument("inputFile")
                    .help("The input class or jar file")
                    .type(new FileArgumentType().verifyExists().verifyIsFile());
            parser.addArgument("outputFile")
                    .help("The output class or jar file")
                    .type(new FileArgumentType());
            parser.addArgument("mappingsFile")
                    .help("The mappings file to remap the classes with")
                    .type(new FileArgumentType().verifyExists().verifyIsFile());
            Namespace namespace = parser.parseArgsOrFail(args);
            File inputFile = namespace.get("inputFile");
            final boolean jarFile;
            String extension = Files.getFileExtension(inputFile.getName());
            switch (extension) {
                case "jar":
                    jarFile = true;
                    break;
                case "class":
                    jarFile = false;
                    break;
                default:
                    throw new CommandException("Unknown file extension: " + extension);
            }
            File outputFile = namespace.get("outputFile");
            File mappingsFile = namespace.get("mappingsFile");
            System.out.println("Loading mappings");
            FastMappings mappings = FastMappings.fromFile(mappingsFile);
            if (!jarFile) throw new UnsupportedOperationException("TODO: Support classfile remapping");
            System.out.println("Remapping jar...");
            try (JarFile inputJar = new JarFile(inputFile)) {
                new JarRemapper(mappings).parallelRemapJar(inputJar, outputFile, Runtime.getRuntime().availableProcessors());
            }
        });
    }
}
