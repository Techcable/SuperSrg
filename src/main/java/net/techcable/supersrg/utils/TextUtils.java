package net.techcable.supersrg.utils;

import com.google.common.base.Preconditions;
import com.google.common.base.Verify;

public class TextUtils {
    private TextUtils() {}
    public static boolean isAsciiWhitespace(char c) {
        // https://infra.spec.whatwg.org/#ascii-whitespace
        switch (c) {
            case '\t':
            case '\n':
            case '\f':
            case '\r':
            case ' ':
                return true;
            default:
                return false;
        }
    }
    public static String trimAsciiWhitespace(String text) {
        int length = text.length();
        if (length < 1 || (!isAsciiWhitespace(text.charAt(0)) && !isAsciiWhitespace(text.charAt(length - 1)))) {
            return text;
        }
        int trimmedStart = 0;
        while (trimmedStart < text.length() && isAsciiWhitespace(text.charAt(trimmedStart))) {
            trimmedStart += 1;
        }
        if (trimmedStart == text.length()) return "";
        int trimmedEnd = length - 1;
        while (trimmedEnd >= 0 && isAsciiWhitespace(text.charAt(trimmedEnd))) {
            trimmedEnd -= 1;
        }
        Verify.verify(trimmedEnd > trimmedStart);
        return text.substring(trimmedStart, trimmedEnd);
    }
    public static boolean isAsciiWord(char c) {
        return c >= 'A' && c <= 'z' && (c <= 'Z' || c >= 'a');
    }
    public static int indexOfNonword(String text, int fromIndex) {
        for (int index = fromIndex; index < text.length(); index++) {
            char c = text.charAt(index);
            if (!isAsciiWord(c)) {
                return index;
            }
        }
        return -1;
    }
    public static int findClosingDelimiter(String text, int index, char open, char close) {
        Preconditions.checkArgument(text.charAt(index) == open, "Char at %s should be open: %s", index, text);
        int level = 1;
        while (++index < text.length()) {
            char c = text.charAt(index);
            if (c == open) {
                level += 1;
            } else if (c == close) {
                if (--level == 0) {
                    return index;
                }
                Verify.verify(level > 0);
            }
        }
        throw new IllegalArgumentException("Unclosed delemiter at " + index + ": " + text);
    }
}
