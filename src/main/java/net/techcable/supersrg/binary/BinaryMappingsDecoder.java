package net.techcable.supersrg.binary;

import java.io.BufferedInputStream;
import java.io.Closeable;
import java.io.DataInput;
import java.io.DataInputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.Objects;
import java.util.zip.ZipException;

import com.google.common.base.Charsets;
import com.google.common.collect.ImmutableMap;
import com.google.common.collect.ImmutableTable;

import io.netty.buffer.ByteBuf;
import io.netty.buffer.ByteBufUtil;
import io.netty.buffer.Unpooled;

import net.jpountz.lz4.LZ4Exception;
import net.techcable.supersrg.utils.CompressionFormat;
import net.techcable.supersrg.utils.FastMappings;
import net.techcable.supersrg.utils.SerializationException;
import net.techcable.supersrg.utils.SerializationUtils;

public class BinaryMappingsDecoder implements Closeable {
    private InputStream input;
    private boolean eof = false;

    public BinaryMappingsDecoder(InputStream input) {
        this.input = Objects.requireNonNull(input);
    }
    private static byte[] EXPECTED_HEADER = "SuperSrg binary mappings".getBytes(StandardCharsets.UTF_8);
    public FastMappings decode() throws IOException, BinaryMappingsException {
        try {
            DataInput dataInput = new DataInputStream(this.input);
            byte[] header = new byte[EXPECTED_HEADER.length];
            dataInput.readFully(header);
            if (!Arrays.equals(header, EXPECTED_HEADER)) {
                throw new BinaryMappingsException("Unexpected header: " + ByteBufUtil.hexDump(header));
            }
            if (input.read() != 0) {
                throw new BinaryMappingsException("Expected a null terminator after header!");
            }
            long version = Integer.toUnsignedLong(dataInput.readInt());
            if (version != 1) {
                throw new BinaryMappingsException("Unexpected version: " + version);
            }
            int compressionLength = dataInput.readUnsignedShort();
            byte[] compressionBytes = new byte[compressionLength];
            dataInput.readFully(compressionBytes);
            String compression = new String(compressionBytes, Charsets.UTF_8);
            if (!compression.isEmpty()) {
                final CompressionFormat compressionFormat;
                switch (compression) {
                    case "lz4-frame":
                        compressionFormat = CompressionFormat.LZ4_FRAMED;
                        break;
                    case "gzip":
                        compressionFormat = CompressionFormat.GZIP;
                        break;
                    case "lzma2":
                        throw new BinaryMappingsException("Unsupported compression: " + compression);
                    default:
                        throw new BinaryMappingsException("Forbidden compression: " + compression);
                }
                input = compressionFormat.createInputStream(input);
            }
            ByteBuf buffer = Unpooled.buffer();
            SerializationUtils.readFully(input, buffer);
            int numClasses = Math.toIntExact(buffer.readUnsignedInt());
            ImmutableMap.Builder<String, FastMappings.ClassMappings> classes = ImmutableMap.builder();
            for (int classNum = 0; classNum < numClasses; classNum++) {
                String originalClassName = SerializationUtils.readPrefixedString(buffer);
                String revisedClassName = SerializationUtils.readPrefixedString(buffer);
                int numMethods = Math.toIntExact(buffer.readUnsignedInt());
                ImmutableTable.Builder<String, String, String> methods = ImmutableTable.builder();
                for (int i = 0; i < numMethods; i++) {
                    String originalName = SerializationUtils.readPrefixedString(buffer);
                    String revisedName = SerializationUtils.readPrefixedString(buffer);
                    if (revisedName.isEmpty()) continue;
                    String originalDescriptor = SerializationUtils.readPrefixedString(buffer);
                    SerializationUtils.readPrefixedString(buffer); // Ignore revised signature
                    methods.put(originalName, originalDescriptor, revisedName);
                }
                int numFields = Math.toIntExact(buffer.readUnsignedInt());
                ImmutableMap.Builder<String, String> fields = ImmutableMap.builder();
                for (int i = 0; i < numFields; i++) {
                    String originalName = SerializationUtils.readPrefixedString(buffer);
                    String revisedName = SerializationUtils.readPrefixedString(buffer);
                    fields.put(originalName, revisedName);
                }
                classes.put(originalClassName, new FastMappings.ClassMappings(
                        originalClassName,
                        revisedClassName.isEmpty() ? null : revisedClassName,
                        fields.build(),
                        methods.build()
                ));
            }
            return new FastMappings(classes.build());
        } catch (SerializationException e) {
            throw new BinaryMappingsException(e);
        } catch (LZ4Exception | ZipException e) {
            throw new BinaryMappingsException("Invalid compressed data", e);
        }
    }

    @Override
    public void close() throws IOException {
        input.close();
    }

    public static FastMappings parseFile(File f) throws IOException, BinaryMappingsException {
       try (BinaryMappingsDecoder decoder = new BinaryMappingsDecoder(new BufferedInputStream(new FileInputStream(f)))) {
           return decoder.decode();
       }
    }
}
