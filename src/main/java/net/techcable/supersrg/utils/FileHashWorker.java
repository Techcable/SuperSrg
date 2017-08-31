package net.techcable.supersrg.utils;

import lombok.*;

import java.io.File;
import java.io.FileInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.Callable;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.atomic.AtomicBoolean;
import javax.annotation.Nonnull;
import javax.annotation.Nullable;

import com.google.common.collect.ImmutableMap;
import com.google.common.util.concurrent.ThreadFactoryBuilder;

import net.techcable.supersrg.cmd.CmdUtils;

@RequiredArgsConstructor(access = AccessLevel.PRIVATE)
public final class FileHashWorker implements Callable<List<FileHashWorker.FileHash>> {
    private final MessageDigest algorithm;
    private final BlockingQueue<File> fileQueue;
    private final AtomicBoolean done;

    private static int numThreads = 0;
    @Nullable
    private static volatile ExecutorService executor;
    private static int getNumThreads() {
        int numThreads = FileHashWorker.numThreads;
        if (numThreads == 0) {
            numThreads = Math.max(2, Runtime.getRuntime().availableProcessors());
        }
        return numThreads;
    }
    @Nonnull
    private static ExecutorService getExecutor() {
        ExecutorService executor = FileHashWorker.executor;
        if (executor == null) {
            synchronized (FileHashWorker.class) {
                executor = FileHashWorker.executor;
                if (executor == null) {
                    executor = Executors.newFixedThreadPool(
                            getNumThreads(),
                            new ThreadFactoryBuilder()
                                .setNameFormat("File hasher #%d")
                                .setDaemon(true)
                                .build()
                    );
                    FileHashWorker.executor = executor;
                }
            }
        }
        return executor;
    }


    @Override
    public List<FileHash> call() throws IOException {
        List<FileHash> result = new ArrayList<>();
        byte[] buffer = new byte[4096];
        File file;
        while ((file = fileQueue.poll()) != null) {
            algorithm.reset();
            try (InputStream in = new FileInputStream(file)) {
                int numBytes;
                while ((numBytes = in.read(buffer)) >= 0) {
                    algorithm.update(buffer, 0, numBytes);
                }
                result.add(new FileHash(file, algorithm.digest()));
            }
        }
        return result;
    }
    @SneakyThrows
    public static ImmutableMap<File, byte[]> hashFiles(String algorithm, File dir) {
        List<Future<List<FileHash>>> futures = new ArrayList<>();
        BlockingQueue<File> fileQueue = new LinkedBlockingQueue<>(CmdUtils.recursivelyListFiles(dir));
        AtomicBoolean done = new AtomicBoolean(false);
        for (int i = 0; i < getNumThreads(); i++) {
            MessageDigest messageDigest = MessageDigest.getInstance(algorithm);
            futures.add(getExecutor().submit(new FileHashWorker(
                    messageDigest,
                    fileQueue,
                    done
            )));
        }
        done.set(true);
        ImmutableMap.Builder<File, byte[]> resultBuilder = ImmutableMap.builder();
        for (Future<List<FileHash>> future : futures) {
            while (true) {
                try {
                    resultBuilder.putAll(future.get());
                    break;
                } catch (InterruptedException ignored) {
                } catch (ExecutionException e) {
                    throw e.getCause();
                }
            }
        }
        return resultBuilder.build();
    }
    @RequiredArgsConstructor
    static class FileHash implements Map.Entry<File, byte[]> {
        /* package */ final File file;
        /* package */ final byte[] hash;

        @Override
        public File getKey() {
            return file;
        }

        @Override
        public byte[] getValue() {
            return hash;
        }

        @Override
        public byte[] setValue(byte[] value) {
            throw new UnsupportedOperationException();
        }
    }
}
