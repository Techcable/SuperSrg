package net.techcable.supersrg.cmd;

import lombok.*;

import java.io.File;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

import com.google.common.base.Verify;
import com.google.common.collect.ImmutableList;

public class CmdUtils {
    private CmdUtils() {}

    public static List<File> splitFiles(String fileList) {
        String[] split = fileList.split(":");
        List<File> files = new ArrayList<>(split.length);
        for (String fileName : split) {
            File file = new File(fileName);
            if (!file.exists()) throw new CommandException("File not found: " + fileName);
            files.add(file);
        }
        return files;
    }


    @SuppressWarnings("UseOfSystemOutOrSystemErr") // Ur mum needs a better logging famework
    @SafeVarargs
    @SneakyThrows
    public static void catchExceptions(CheckedRunnable action, Class<? extends Throwable>... errorTypes) {
        try {
            action.run();
        } catch (CommandException e) {
            e.getMessages().forEach(System.err::println);
            System.exit(1);
        } catch (Throwable t) {
            for (Class<?> handledErrorType : errorTypes) {
                if (handledErrorType.isInstance(t)) {
                    System.err.println(t.getClass().getSimpleName() + ": " + t.getMessage());
                    System.exit(1);
                }
            }
            throw t;
        }
    }

    public static List<File> recursivelyListFiles(File f) throws IOException {
        return Files.walk(f.toPath())
                .filter(Files::isRegularFile)
                .map(Path::toFile)
                .collect(ImmutableList.toImmutableList());
    }
    public static String relativePath(File target, File dir) {
        String absoluteTarget = target.getAbsolutePath();
        String absoluteDir = dir.getAbsolutePath();
        Verify.verify(!absoluteDir.endsWith("/"));
        if (absoluteTarget.length() <= absoluteDir.length()
                || !absoluteTarget.startsWith(absoluteDir)
                || absoluteTarget.charAt(absoluteDir.length()) != File.separatorChar) {
            throw new IllegalArgumentException("Target " + target + " not in directory " + dir);
        }
        return absoluteTarget.substring(absoluteDir.length() + 1);
    }
}
