package net.techcable.supersrg.utils;

import lombok.*;

import java.util.Arrays;
import java.util.Objects;
import java.util.regex.Pattern;
import javax.annotation.Nullable;

import com.google.common.base.Preconditions;
import com.google.common.base.Verify;

/**
 * An immutable set of text to ignore.
 */
public class IgnoreSet {
    @Getter
    private final String text;
    @Nullable
    private final boolean[] ignoreFlags;
    private IgnoreSet(String text, @Nullable boolean[] ignoreFlags) {
        this.text = Objects.requireNonNull(text);
        this.ignoreFlags = ignoreFlags;
        if (ignoreFlags != null && ignoreFlags.length != text.length()) {
            throw new IllegalArgumentException("Unexpected ignore flags " + Arrays.toString(ignoreFlags) + ": " + text);
        }
    }

    public boolean isEmpty() {
        return ignoreFlags == null;
    }

    public boolean isIgnored(int index) {
        Preconditions.checkElementIndex(index, textLength());
        return ignoreFlags != null && ignoreFlags[index];
    }

    public int textLength() {
        Verify.verify(ignoreFlags == null || text.length() == ignoreFlags.length);
        return text.length();
    }

    /**
     * Find the index of the specified string, ignoring the sections in this set.
     *
     * @param str the string to look for
     * @param fromIndex the index to start searching from
     * @return the first index of the given string after the start index, or -1 if not found.
     */
    public int ignoringIndexOf(String str, int fromIndex) {
        final boolean[] commentFlags = this.ignoreFlags;
        if (commentFlags == null) {
            return text.indexOf(str, fromIndex);
        }
        int index = fromIndex;
        indexLoop: do {
            index = text.indexOf(str, index);
            if (index < 0) {
                return -1;
            } else if (commentFlags[index]) {
                while (++index < commentFlags.length) {
                    if (!commentFlags[index]) {
                        continue indexLoop;
                    }
                }
                return -1;
            } else {
                return index;
            }
        } while (true);
    }

    public String stripIgnored() {
        boolean[] ignoreFlags = this.ignoreFlags;
        if (ignoreFlags == null) return text;
        StringBuilder result = new StringBuilder(text.length());

        for (int i = 0; i < text.length(); i++) {
            if (!isIgnored(i)) {
                result.append(text.charAt(i));
            }
        }
        return result.toString();
    }

    public IgnoreSet union(IgnoreSet other) {
        boolean[] thisFlags = this.ignoreFlags;
        boolean[] otherFlags = other.ignoreFlags;
        int thisLength = thisFlags != null ? thisFlags.length : this.textLength();
        int otherLength = otherFlags != null ? otherFlags.length : other.textLength();
        Preconditions.checkArgument(this.text.equals(other.text), "Different text: %s and %s", this.text, other.text);
        if (otherFlags == null) {
            return this;
        } else if (thisFlags == null) {
            return other;
        } else {
            boolean[] resultFlags = new boolean[thisFlags.length];
            Verify.verify(thisFlags.length == otherFlags.length);
            for (int i = 0; i < resultFlags.length; i++) {
                resultFlags[i] = thisFlags[i] | otherFlags[i];
            }
            return new IgnoreSet(text, resultFlags);
        }
    }

    public static String stripComments(String original) {
        return findComments(original).stripIgnored();
    }

    public static IgnoreSet findAnnotations(String original) {
        ParsedAnnotation annotation = ParsedAnnotation.nextAnnotation(original, 0);
        if (annotation == null) return new IgnoreSet(original, null);
        boolean[] ignoreFlags = new boolean[original.length()];
        do {
            Arrays.fill(ignoreFlags, annotation.getStart(), annotation.getEnd(), true);
            annotation = ParsedAnnotation.nextAnnotation(original, annotation.getEnd());
        } while (annotation != null);
        return new IgnoreSet(original, ignoreFlags);
    }

    public static IgnoreSet findComments(String original) {
        int commentStart = findNextComment(original, 0);
        if (commentStart < 0) return new IgnoreSet(original, null);
        int index;
        boolean[] commentFlags = new boolean[original.length()];
        commentLoop: do {
            char commentType = original.charAt(commentStart + 1);
            switch (commentType) {
                case '/':
                    // For a line comment, skip till the end of line
                    int lineEnd = original.indexOf('\n', commentStart);
                    if (lineEnd < 0) {
                        Arrays.fill(commentFlags, commentStart, original.length(), true);
                        break commentLoop;
                    } else {
                        index = lineEnd;
                        Arrays.fill(commentFlags, commentStart, lineEnd, true);
                    }
                    break;
                case '*':
                    int commentEnd = original.indexOf("*/", commentStart);
                    if (commentEnd < 0) {
                        throw new IllegalArgumentException("Unclosed comment @ " + commentStart + ": " + original);
                    }
                    Arrays.fill(commentFlags, commentStart, commentEnd + 2, true);
                    index = commentEnd + 2;
                    break;
                default:
                    throw new AssertionError(commentType);
            }
            commentStart = findNextComment(original, index);
        } while (commentStart >= 0);
        return new IgnoreSet(original, commentFlags);
    }
    public static int findNextComment(String text, int fromIndex) {
        int index = fromIndex;
        while ((index = text.indexOf('/', index)) >= 0 && index + 1 < text.length()) {
            char nextChar = text.charAt(index + 1);
            if (nextChar == '/' || nextChar == '*') {
                return index;
            }
            index += 1;
        }
        return -1;
    }
}
