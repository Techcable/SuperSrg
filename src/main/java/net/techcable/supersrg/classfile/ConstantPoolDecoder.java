package net.techcable.supersrg.classfile;

import lombok.*;

import java.io.Closeable;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.Objects;
import javax.annotation.Nonnull;

import com.google.common.base.Preconditions;
import com.google.common.base.Verify;

import io.netty.buffer.ByteBuf;

public class ConstantPoolDecoder implements Closeable {
    private ByteBuf buffer;
    private final byte[] tags;
    private final int[] offsets;
    private String[] stringCache;
    @Getter
    private final int start, end, version;
    private ConstantPoolDecoder(ByteBuf buffer, int[] offsets, byte[] tags, int start, int end, int version) {
        Preconditions.checkArgument(version <= 52, "Invalid version: %s", version);
        this.buffer = buffer.retain();
        this.offsets = Objects.requireNonNull(offsets);
        this.tags = Objects.requireNonNull(tags);
        Verify.verify(offsets.length == tags.length);
        Verify.verify(end >= start);
        this.start = start;
        this.end = end;
        this.version = version;
    }

    public ByteBuf getBuffer() {
        ByteBuf buffer = this.buffer;
        if (buffer == null) throw new IllegalStateException();
        return buffer;
    }
    public int size() {
        return tags.length;
    }
    public int byteSize() {
        return end - start;
    }
    public byte getTag(int index) {
        byte[] tags = this.tags;
        if (index < 0 || index >= tags.length) {
            throw new IndexOutOfBoundsException(invalidIndex(index, tags.length));
        }
        return tags[index];
    }
    public int getOffset(int index) {
        if (index < 0 || index >= this.size()) {
            throw new IndexOutOfBoundsException(invalidIndex(index, this.size()));
        }
        return offsets[index];
    }
    @Nonnull
    public String getUnicode(int index) throws ConstantPoolDecodeException {
        this.checkTag(index, 1);
        String[] stringCache = this.stringCache;
        String result;
        if (stringCache == null || (result = stringCache[index]) == null) {
            if (stringCache == null) {
                this.stringCache = stringCache = new String[this.size()];
            }
            result = stringCache[index] = decodeUnicode(index);
        }
        return result;
    }
    /**
     * Return the name and type descriptor at the specified index as a packed int.
     */
    public int getNameAndTypeDescriptor(int index) throws ConstantPoolDecodeException {
        this.checkTag(index, NAME_AND_TYPE_DESCRIPTOR_TAG);
        return buffer.getInt(this.offsets[index]);
    }
    private String decodeUnicode(int index) throws ConstantPoolDecodeException {
        this.checkTag(index, 1);
        int offset = offsets[index];
        int length = buffer.getUnsignedShort(offset);
        return buffer.toString(offset + 2, length, StandardCharsets.UTF_8);
    }

    private void checkTag(int index, int expected) throws ConstantPoolDecodeException {
        byte[] tags = this.tags;
        if (index < 0 || index >= tags.length) {
            throw new ConstantPoolDecodeException(invalidIndex(index, tags.length));
        }
        byte actual = tags[index];
        if (actual != expected) {
            throw new ConstantPoolDecodeException(unexpectedTag(index, expected, actual));
        }
    }
    private static String unexpectedTag(int index, int expected, byte actual) {
        return "Expected tag " + expected + " at " + index + ", but got " + actual;
    }
    private static String invalidIndex(int index, int length) {
        if (index < 0) {
            return "Negative index: " + index;
        } else {
            return "Index " + index + " out of bounds for " + length + " element constant pool";
        }
    }

    @Override
    public void close() throws IOException {
        if (buffer != null) {
            buffer.release();
            buffer = null;
        }
    }

    public static ConstantPoolDecoder decode(ByteBuf data) throws ConstantPoolDecodeException {
        int start = data.readerIndex();
        int header = data.readInt();
        if (header != 0xCAFEBABE) {
            throw new ConstantPoolDecodeException("Invalid header: " + Integer.toHexString(header));
        }
        data.readUnsignedShort(); // Ignore minor version
        int version = data.readUnsignedShort();
        if (version > 52) {
            throw new ConstantPoolDecodeException("Unsupported version: " + version);
        }
        int constantPoolCount = data.readUnsignedShort();
        if (constantPoolCount < 1) {
            throw new ConstantPoolDecodeException("Invalid constant pool 'count': " + constantPoolCount);
        }
        int constantPoolSize = constantPoolCount - 1;
        byte[] tags = new byte[constantPoolSize];
        int[] offsets = new int[constantPoolSize];
        for (int index = 0; index < constantPoolSize; index++) {
            byte tag = data.readByte();
            int offset = data.readerIndex();
            offsets[index] = offset;
            tags[index] = tag;
            switch (tag) {
                case UNICODE_TAG:
                    int length = data.readUnsignedShort();
                    data.skipBytes(length);
                    break;
                case LONG_TAG:
                case DOUBLE_TAG:
                    // Long and double take two indexes
                    index += 1;
                default:
                    data.skipBytes(sizeOf(tag));
                    break;
            }
        }
        int end = data.readerIndex();
        return new ConstantPoolDecoder(data, offsets, tags, start, end, version);
    }

    // Tag ids
    public static final byte UNICODE_TAG = 1;
    public static final byte INTEGER_TAG = 3;
    public static final byte FLOAT_TAG = 4;
    public static final byte LONG_TAG = 5;
    public static final byte DOUBLE_TAG = 6;
    public static final byte CLASS_REFERENCE_TAG = 7;
    public static final byte STRING_REFERENCE_TAG = 8;
    public static final byte FIELD_REFERENCE_TAG = 9;
    public static final byte METHOD_REFERENCE_TAG = 10;
    public static final byte INTERFACE_METHOD_REFERENCE_TAG = 11;
    public static final byte NAME_AND_TYPE_DESCRIPTOR_TAG = 12;
    public static final byte METHOD_HANDLE_TAG = 15;
    public static final byte METHOD_TYPE_TAG = 16;
    public static final byte INVOKEDYNAMIC_TAG = 18;
    private static final int[] SIZE_TABLE;
    static {
        SIZE_TABLE = new int[19];
        for (int id = 0; id < SIZE_TABLE.length; id++) {
            final int size;
            switch (id) {
                case LONG_TAG:
                case DOUBLE_TAG:
                    size = 8;
                    break;
                case CLASS_REFERENCE_TAG:
                case STRING_REFERENCE_TAG:
                case METHOD_TYPE_TAG:
                    size = 2;
                    break;
                case INTEGER_TAG:
                case FLOAT_TAG:
                case FIELD_REFERENCE_TAG:
                case METHOD_REFERENCE_TAG:
                case INTERFACE_METHOD_REFERENCE_TAG:
                case NAME_AND_TYPE_DESCRIPTOR_TAG:
                case INVOKEDYNAMIC_TAG:
                    size = 4;
                    break;
                case METHOD_HANDLE_TAG:
                    size = 3;
                    break;
                case UNICODE_TAG: // Unicode tag is considered an unknown size
                default:
                    size = -1;
                    break;
            }
            SIZE_TABLE[id] = size;
        }
    }
    public static int sizeOf(byte tag) {
        int result = tag >= 0 && tag < SIZE_TABLE.length ? SIZE_TABLE[tag] : -1;
        if (result < 0) throw new IllegalArgumentException("Unknown size for " + tag);
        return result;
    }
}
