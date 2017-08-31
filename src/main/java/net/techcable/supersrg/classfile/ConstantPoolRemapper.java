package net.techcable.supersrg.classfile;

import java.io.Closeable;
import java.io.IOException;
import java.util.Arrays;
import java.util.Objects;

import com.google.common.base.Preconditions;
import com.google.common.base.Verify;

import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;

import net.techcable.supersrg.utils.FastMappings;
import net.techcable.supersrg.utils.SerializationUtils;

import static net.techcable.supersrg.classfile.ConstantPoolDecoder.*;

public class ConstantPoolRemapper implements Closeable {
    private final FastMappings mappings;
    private final ConstantPoolDecoder decoder;
    private final boolean[] hasClassMappings;
    private final FastMappings.ClassMappings[] classMappingTable;
    private final int[] remappedDescriptorTable;
    private ByteBuf out;
    private final ByteBuf additionalConstants;
    private int numAdditionalConstants = 0;

    public ConstantPoolRemapper(FastMappings mappings, ConstantPoolDecoder decoder, ByteBuf out) {
        this.mappings = Objects.requireNonNull(mappings);
        this.decoder = Objects.requireNonNull(decoder);
        this.out = Objects.requireNonNull(out).retain();
        this.hasClassMappings = new boolean[decoder.size()];
        this.classMappingTable = new FastMappings.ClassMappings[decoder.size()];
        this.remappedDescriptorTable = new int[decoder.size()];
        Arrays.fill(remappedDescriptorTable, -1);
        this.additionalConstants = Unpooled.buffer(decoder.byteSize());
    }

    private FastMappings.ClassMappings getClassMappings(int classReference) throws ConstantPoolDecodeException {
        FastMappings.ClassMappings classMappings = classMappingTable[classReference];
        if (classMappings == null && !hasClassMappings[classReference]) {
            int classNameIndex = decoder.getBuffer().getUnsignedShort(decoder.getOffset(classReference));
            String className = decoder.getUnicode(classNameIndex - 1);
            classMappings = classMappingTable[classReference] = mappings.getClassMappings(className);
            hasClassMappings[classReference] = true;
        }
        return classMappings;
    }

    public int insertNameAndTypeDescriptor(int name, int typeDescriptor) {
        additionalConstants.writeByte(NAME_AND_TYPE_DESCRIPTOR_TAG);
        additionalConstants.writeShort(name);
        additionalConstants.writeShort(typeDescriptor);
        return nextAdditionalConstantIndex();
    }
    public int remapTypeDescriptor(int index) throws ConstantPoolDecodeException {
        int remappedDescriptorIndex = remappedDescriptorTable[index];
        if (remappedDescriptorIndex < 0) {
            String originalDescriptor = decoder.getUnicode(index);
            String remappedDescriptor = mappings.remapTypeDescriptor(originalDescriptor);
            if (remappedDescriptor != null) {
                remappedDescriptorIndex = insertUnicode(remappedDescriptor);
            } else {
                remappedDescriptorIndex = index;
            }
            remappedDescriptorTable[index] = remappedDescriptorIndex;
        }
        return remappedDescriptorIndex;
    }
    public int remapMethodDescriptor(int index) throws ConstantPoolDecodeException {
        int remappedDescriptorIndex = remappedDescriptorTable[index];
        if (remappedDescriptorIndex < 0) {
            String originalDescriptor = decoder.getUnicode(index);
            String remappedDescriptor = mappings.remapMethodDescriptor(originalDescriptor);
            if (remappedDescriptor != null) {
                remappedDescriptorIndex = insertUnicode(remappedDescriptor);
            } else {
                remappedDescriptorIndex = index;
            }
            remappedDescriptorTable[index] = remappedDescriptorIndex;
        }
        return remappedDescriptorIndex;
    }
    public int insertUnicode(String value) {
        Objects.requireNonNull(value);
        additionalConstants.writeByte(UNICODE_TAG);
        SerializationUtils.writePrefixedString(additionalConstants, value);
        return nextAdditionalConstantIndex();
    }
    private int nextAdditionalConstantIndex() {
        int resultIndex = decoder.size() + numAdditionalConstants++;
        Verify.verify(resultIndex >= 0);
        return resultIndex;
    }
    public void remap(ByteBuf out) throws ConstantPoolDecodeException {
        out.writeInt(0xCAFEBABE);
        out.writeShort(0);
        out.writeShort(decoder.getVersion());
        int countIndex = out.writerIndex();
        out.writeShort(decoder.size() + 1); // Placeholder
        ByteBuf inBuf = decoder.getBuffer();
        // TODO: Avoid placing duplicate data in the resulting constant pool with a HashMap
        boolean[] hasClassMappings = new boolean[decoder.size()];
        for (int index = 0; index < decoder.size(); index++) {
            byte tag = decoder.getTag(index);
            int offset = decoder.getOffset(index);
            switch (tag) {
                case FIELD_REFERENCE_TAG:
                case METHOD_REFERENCE_TAG:
                case INTERFACE_METHOD_REFERENCE_TAG: {
                    // Since the classes are remapped independently, all we have to handle is the field names
                    int classReference = inBuf.getUnsignedShort(offset);
                    FastMappings.ClassMappings classMappings = this.getClassMappings(classReference - 1);
                    if (classMappings != null) {
                        int nameAndTypeDescriptor = decoder.getNameAndTypeDescriptor(inBuf.getUnsignedShort(offset + 2));
                        int nameIndex = SerializationUtils.firstUnsignedShort(nameAndTypeDescriptor);
                        int originalDescriptor = SerializationUtils.secondUnsignedShort(nameAndTypeDescriptor);
                        String originalName = decoder.getUnicode(nameIndex - 1);
                        final String newName;
                        final int remappedDescriptor;
                        if (tag == FIELD_REFERENCE_TAG) {
                            newName = classMappings.getFieldName(originalName);
                            remappedDescriptor = remapTypeDescriptor(originalDescriptor - 1);
                        } else {
                            String originalDescriptorText = decoder.getUnicode(originalDescriptor - 1);
                            newName = classMappings.getMethodName(originalName, originalDescriptorText);
                            remappedDescriptor = remapMethodDescriptor(originalDescriptor - 1);
                        }
                        if (newName != null || remappedDescriptor != originalDescriptor) {
                            int remappedNameAndTypeDescriptor = insertNameAndTypeDescriptor(
                                    newName != null ? insertUnicode(newName) + 1 : nameIndex,
                                    remappedDescriptor + 1
                            );
                            out.writeByte(tag);
                            out.writeShort(classReference + 1);
                            out.writeShort(remappedNameAndTypeDescriptor + 1);
                            break;
                        }
                    }
                    // Since we haven't remapped anything, just skip over the bytes
                    out.writeByte(tag);
                    out.writeBytes(inBuf, offset, ConstantPoolDecoder.sizeOf(tag));
                    break;
                }
                case METHOD_TYPE_TAG: {
                    int originalDescriptor = inBuf.getUnsignedShort(offset);
                    out.writeByte(tag);
                    out.writeByte(remapMethodDescriptor(originalDescriptor - 1) + 1);
                    break;
                }
                case CLASS_REFERENCE_TAG: {
                    FastMappings.ClassMappings classMappings = getClassMappings(index);
                    if (classMappings != null && classMappings.getRemappedName() != null) {
                        out.writeByte(tag);
                        out.writeShort(insertUnicode(classMappings.getRemappedName()) + 1);
                    } else {
                        out.writeByte(tag);
                        out.writeBytes(inBuf, offset, ConstantPoolDecoder.sizeOf(tag));
                    }
                    break;
                }
                case UNICODE_TAG:
                    // The unicode tag is special, and we have to handle its variable size
                    // NOTE: We don't actually decode the underlying string for performance,
                    // and just copy the associated bytes
                    out.writeByte(1);
                    int length = inBuf.getUnsignedShort(offset);
                    out.writeBytes(inBuf, offset + 2, length);
                    break;
                case DOUBLE_TAG:
                case LONG_TAG:
                    // Double and long take two indexes
                    index += 1;
                case INVOKEDYNAMIC_TAG:
                    // invokedynamic calls can be safely ignored, as their corresponding method references are already remapped
                case METHOD_HANDLE_TAG:
                    // We can safely ignore, as the corresponding field and method references are already handled
                case NAME_AND_TYPE_DESCRIPTOR_TAG:
                    // Name and type descriptors need class information to remap properly,
                    // so wait until it's used in a field or method reference
                case STRING_REFERENCE_TAG:
                case FLOAT_TAG:
                case INTEGER_TAG:
                    // Ignored, just write however many bytes they take
                    out.writeByte(tag);
                    out.writeBytes(inBuf, offset, ConstantPoolDecoder.sizeOf(tag));
                    break;
                default:
                    throw new UnsupportedOperationException("Unknown tag: " + tag);
            }
        }
        out.setShort(countIndex, decoder.size() + numAdditionalConstants + 1);
        out.writeBytes(this.additionalConstants);
    }

    @Override
    public void close() throws IOException {
        if (out != null) {
            out.release();
            out = null;
        }
    }
}
