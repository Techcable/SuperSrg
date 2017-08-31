package net.techcable.supersrg.source;

import lombok.*;

import java.io.BufferedInputStream;
import java.io.File;
import java.io.FileInputStream;
import java.util.Arrays;
import java.util.HashMap;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentMap;
import java.util.concurrent.ExecutionException;
import javax.annotation.Nonnull;

import com.google.common.base.Preconditions;
import com.google.common.cache.CacheBuilder;
import com.google.common.cache.CacheLoader;
import com.google.common.cache.LoadingCache;

import io.netty.buffer.ByteBuf;

@Getter
@EqualsAndHashCode
public class FileLocation implements Comparable<FileLocation> {
    private final int start, end;
    public FileLocation(int start, int end) {
        Preconditions.checkArgument(start >= 0, "Negative start: %s", start);
        Preconditions.checkArgument(end >= 0, "Negative end: %s", end);
        Preconditions.checkArgument(end >= start, "Start %s greater than end %s", start, end);
        this.start = start;
        this.end = end;
    }

    public static FileLocation parse(String s) {
        int seperatorIndex = s.indexOf(':');
        if (seperatorIndex < 0) {
            throw new IllegalArgumentException("Invalid file location: " + s);
        }
        int start = Integer.parseInt(s.substring(0, seperatorIndex));
        int end = Integer.parseInt(s.substring(seperatorIndex + 1));
        return new FileLocation(start, end);
    }

    public boolean hasOverlap(FileLocation other) {
        if (this.start == other.start) return true;
        if (this.start > other.start) {
            // other, this
            return other.end > this.start;
        } else {
            // this, other
            return this.end > other.start;
        }
    }

    public boolean isEmpty() {
        return start == end;
    }
    public int size() {
        return end - start;
    }

    public void serialize(ByteBuf out) {
        out.writeInt(start);
        out.writeInt(end);
    }

    public static FileLocation deserialize(ByteBuf in) {
        return new FileLocation(
                in.readInt(),
                in.readInt()
        );
    }

    @Override
    public String toString() {
        return Integer.toString(start) + ':' + Integer.toString(end);
    }

    @Override
    public int compareTo(@Nonnull FileLocation other) {
        if (this.start != other.start) return Integer.compare(this.start, other.start);
        return Integer.compare(this.end, other.end);
    }
    private int lineNumber;
    private int getLineNumber(File file) {
        int lineNumber = this.lineNumber;
        if (lineNumber == 0) {
            this.lineNumber = lineNumber = determineLine(file);
        }
        return lineNumber;
    }


    private static final LoadingCache<File, int[]> lineOffsets = CacheBuilder.newBuilder()
            .softValues()
            .maximumSize(500)
            .build(new LineOffsetLoader());
    @SneakyThrows
    private int determineLine(File file) {
        try {
            int[] offsets = lineOffsets.get(file);
            for (int lineIndex = 0; lineIndex < offsets.length; lineIndex++) {
                int offset = offsets[lineIndex];
                if (start < offset) {
                    return lineIndex; // NOTE: This is fine, since lines are one-indexed
                }
            }
            return offsets.length;
        } catch (ExecutionException e) {
            throw e.getCause();
        }
    }
    private static class LineOffsetLoader extends CacheLoader<File, int[]> {
        @Override
        public int[] load(@Nonnull File key) throws Exception {
            try (BufferedInputStream in = new BufferedInputStream(new FileInputStream(key))) {
                int[] lineOffsets = new int[256];
                int numLines = 0;
                int fileIndex = 0;
                int b;
                lineOffsets[numLines++] = 0;
                while ((b = in.read()) >= 0) {
                    fileIndex += 1;
                    if (b == '\n') {
                        if (numLines >= lineOffsets.length) {
                            lineOffsets = Arrays.copyOf(lineOffsets, lineOffsets.length * 2);
                        }
                        lineOffsets[numLines++] = fileIndex;
                    }
                }
                return Arrays.copyOf(lineOffsets, numLines);
            }
        }
    }
}
