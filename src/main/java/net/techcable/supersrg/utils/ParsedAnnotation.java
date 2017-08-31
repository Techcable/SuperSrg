package net.techcable.supersrg.utils;

import lombok.*;

import javax.annotation.Nullable;

@RequiredArgsConstructor(access = AccessLevel.PRIVATE)
@Getter
public class ParsedAnnotation {
    private final String originalSource;
    private final int nameStart, nameEnd;
    private final int paramsEnd;
    private final String annotationName;

    public int getStart() {
        return nameStart - 1;
    }
    public int getEnd() {
        return paramsEnd >= 0 ? paramsEnd : nameEnd;
    }

    @Override
    public String toString() {
        return originalSource.substring(getStart(), getEnd());
    }

    @Nullable
    public static ParsedAnnotation nextAnnotation(String text, int fromIndex) {
        int index = fromIndex;
        int potentialAnnotation;
        do {
            potentialAnnotation = text.indexOf('@', index);
            if (potentialAnnotation < 0) return null;
            int nameEnd = TextUtils.indexOfNonword(text, potentialAnnotation + 1);
            String annotationName = text.substring(potentialAnnotation + 1, nameEnd);
            if (annotationName.isEmpty()) continue;
            if (nameEnd < text.length() && text.charAt(nameEnd) == '(') {
                int paramEnd = TextUtils.findClosingDelimiter(text, nameEnd, '(', ')');
                return new ParsedAnnotation(text, potentialAnnotation + 1, nameEnd, paramEnd + 1, annotationName);
            } else {
                return new ParsedAnnotation(text, potentialAnnotation + 1, nameEnd, -1, annotationName);
            }
        } while (++index < text.length());
        return null;
    }
}
