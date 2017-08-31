package net.techcable.supersrg.cmd;

import lombok.*;

import java.util.StringJoiner;

import com.google.common.collect.ImmutableList;

public class CommandException extends RuntimeException {
    @Getter
    private final ImmutableList<String> messages;
    public CommandException(String message) {
        super(message);
        if (isBlank(message)) throw new IllegalArgumentException("Empty message");
        this.messages = ImmutableList.of(message);
    }
    public CommandException(String... messages) {
        super(joinMessages());
        for (int i = 0; i < messages.length; i++) {
            String message = messages[i];
            if (isBlank(message)) {
                throw new IllegalArgumentException("Message #" + i + " is blank: " + getMessage());
            }
        }
        this.messages = ImmutableList.copyOf(messages);
    }

    private static String joinMessages(String... messages) {
        StringJoiner result = new StringJoiner(", ", "[", "]");
        StringBuilder builder = new StringBuilder();
        for (String message : messages) {
            builder.setLength(0); // Clear existing
            builder.append('"');
            builder.append(message);
            builder.append('"');
            result.add(builder);
        }
        return result.toString();
    }
    private static boolean isBlank(String s) {
        return s.codePoints().allMatch(Character::isWhitespace);
    }
}
