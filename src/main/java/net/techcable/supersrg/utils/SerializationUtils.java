package net.techcable.supersrg.utils;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.nio.channels.FileChannel;
import java.nio.charset.StandardCharsets;
import java.nio.file.StandardOpenOption;

import com.google.common.base.Preconditions;
import com.google.common.base.Verify;
import com.google.common.primitives.UnsignedLongs;

import io.netty.buffer.ByteBuf;
import io.netty.buffer.ByteBufUtil;
import io.netty.buffer.PooledByteBufAllocator;

public class SerializationUtils {
    private SerializationUtils() {}
    public static void writeUnsignedShortExact(ByteBuf out, int value) {
        Preconditions.checkArgument(value >= 0, "Negative value: %s", value);
        short s = (short) value;
        Preconditions.checkArgument(Short.toUnsignedInt(s) == value, "Value overflowed: %s", value);
        out.writeShort(s);
    }
    public static long readUnsignedLongExact(ByteBuf in) throws SerializationException {
        long l = in.readLong();
        if (l < 0) {
            throw new SerializationException("Overflowed long: " + UnsignedLongs.toString(l));
        }
        return l;
    }
    public static void writePrefixedString(ByteBuf out, String s) {
        ByteBuf temp = PooledByteBufAllocator.DEFAULT.buffer(ByteBufUtil.utf8MaxBytes(s));
        temp.writerIndex(0);
        ByteBufUtil.writeUtf8(temp, s);
        temp.readerIndex(0);
        writeUnsignedShortExact(out, temp.readableBytes());
        out.writeBytes(temp);
        temp.release();
    }
    public static String readPrefixedString(ByteBuf in) {
        int length = in.readUnsignedShort();
        String result = in.toString(in.readerIndex(), length, StandardCharsets.UTF_8);
        in.skipBytes(length);
        return result;
    }
    public static int readFully(InputStream in, ByteBuf buffer) throws IOException {
        int totalNumRead = 0;
        while (true) {
            int numRead = buffer.writeBytes(in, 4096);
            if (numRead < 0) return totalNumRead;
            totalNumRead += numRead;
        }
    }
    public static void readFully(InputStream in, ByteBuf buffer, int amount) throws IOException {
        Preconditions.checkArgument(amount >= 0, "Invalid amount: %s", amount);
        int totalNumRead = 0;
        buffer.ensureWritable(amount);
        while (amount < 0 || totalNumRead < amount) {
            int numRead = buffer.writeBytes(in, amount - totalNumRead);
            if (numRead < 0) {
                throw new SerializationException("Unexpected EOF");
            }
            totalNumRead += amount;
        }
        Verify.verify(totalNumRead == amount);
    }
    public static byte[] readNullTerminatedBytes(ByteBuf buf) throws SerializationException {
        int end = buf.indexOf(buf.readerIndex(), buf.writerIndex(), (byte) 0);
        if (end < 0) {
            throw new SerializationException("Unable to find null terminator!");
        }
        byte[] bytes = new byte[end - buf.readerIndex()];
        buf.readBytes(bytes);
        Verify.verify(buf.readByte() == 0);
        return bytes;
    }
    public static void writeToFile(ByteBuf buffer, File target) throws IOException {
        try (FileChannel channel = FileChannel.open(target.toPath(), StandardOpenOption.WRITE, StandardOpenOption.CREATE, StandardOpenOption.TRUNCATE_EXISTING)) {
            buffer.readBytes(channel, buffer.readableBytes());
        }
    }
    public static void loadFromFile(ByteBuf buffer, File target) throws IOException {
        try (FileChannel channel = FileChannel.open(target.toPath(), StandardOpenOption.READ)) {
            int remainingBytes;
            do {
                long oldPosition = channel.position();
                remainingBytes = (int) Math.min(channel.size() - oldPosition, Integer.MAX_VALUE);
                int numRead = buffer.writeBytes(channel, oldPosition, remainingBytes);
                channel.position(oldPosition + numRead);
            } while (remainingBytes > 0);
        }
    }
    public static int firstUnsignedShort(int packed) {
        return packed & 0xFFFF;
    }
    public static int secondUnsignedShort(int packed) {
        return (packed >>> 16) & 0xFFFF;
    }
}
