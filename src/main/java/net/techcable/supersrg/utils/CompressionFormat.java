package net.techcable.supersrg.utils;

import lombok.*;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.ByteBuffer;
import java.util.Objects;
import java.util.zip.GZIPInputStream;
import java.util.zip.GZIPOutputStream;
import java.util.zip.ZipException;

import com.google.common.base.Preconditions;
import com.google.common.base.Verify;

import io.netty.buffer.ByteBuf;
import io.netty.buffer.ByteBufInputStream;
import io.netty.buffer.ByteBufOutputStream;

import net.jpountz.lz4.LZ4BlockInputStream;
import net.jpountz.lz4.LZ4BlockOutputStream;
import net.jpountz.lz4.LZ4Compressor;
import net.jpountz.lz4.LZ4Exception;
import net.jpountz.lz4.LZ4Factory;
import net.jpountz.lz4.LZ4FrameInputStream;
import net.jpountz.lz4.LZ4FrameOutputStream;
import net.jpountz.lz4.LZ4SafeDecompressor;

import org.apache.commons.compress.compressors.lz4.FramedLZ4CompressorInputStream;

public enum CompressionFormat {
    LZ4_BLOCK {
        @Override
        public void compress(ByteBuf decompressed, ByteBuf compressed) throws LZ4Exception {
            LZ4Compressor compressor = LZ4Factory.fastestInstance().fastCompressor();
            compressed.ensureWritable(compressor.maxCompressedLength(decompressed.readableBytes()));
            ByteBuffer nioDecompressed = decompressed.nioBuffer();
            ByteBuffer nioCompressed = compressed.nioBuffer(compressed.writerIndex(), compressed.writableBytes());
            Verify.verify(nioDecompressed.position() == 0 && nioCompressed.position() == 0);
            compressor.compress(nioDecompressed, nioCompressed);
            decompressed.skipBytes(nioDecompressed.position());
            compressed.writerIndex(compressed.writerIndex() + nioCompressed.position());
        }
        @Override
        public void decompress(ByteBuf compressed, ByteBuf decompressed) throws LZ4Exception {
            LZ4SafeDecompressor decompressor = LZ4Factory.fastestInstance().safeDecompressor();
            int compressedLength = compressed.readableBytes();
            Preconditions.checkArgument(compressedLength > 0, "Empty comppressed bytes");
            int decompressedLength = compressedLength * 2;
            decompressed.ensureWritable(decompressedLength);
            do {
                try {
                    ByteBuffer compressedNio = compressed.nioBuffer();
                    ByteBuffer decompressedNio = decompressed.nioBuffer(decompressed.writerIndex(), decompressed.writableBytes());
                    Verify.verify(compressedNio.position() == 0 && decompressedNio.position() == 0);
                    decompressor.decompress(compressedNio, decompressedNio);
                    compressed.skipBytes(compressedNio.position());
                    decompressed.writerIndex(decompressed.writerIndex() + decompressedNio.position());
                    return;
                } catch (LZ4Exception e) {
                    try {
                        decompressedLength *= 2;
                        decompressed.ensureWritable(decompressedLength);
                    } catch (OutOfMemoryError oom) {
                        // Error probably caused by invalid data, not an insufficient buffer
                        throw e;
                    }
                }
            } while (true);
        }
    },
    LZ4_FRAMED,
    GZIP;

    @SneakyThrows(IOException.class) // Operating on in-memory buffers
    public void decompress(ByteBuf compressed, ByteBuf decompressed) throws ZipException, LZ4Exception {
        Objects.requireNonNull(compressed);
        Objects.requireNonNull(decompressed);
        compressed.retain();
        decompressed.retain();
        try {
            InputStream in = createInputStream(new ByteBufInputStream(compressed));
            byte[] buffer = new byte[4096];
            int numRead;
            while ((numRead = in.read(buffer)) >= 0) {
                decompressed.writeBytes(buffer, 0, numRead);
            }
        } finally {
            compressed.release();
            decompressed.release();
        }
    }
    @SneakyThrows(IOException.class) // Operating on in-memory buffers
    public void compress(ByteBuf decompressed, ByteBuf compressed) throws ZipException, LZ4Exception {
        Objects.requireNonNull(compressed);
        Objects.requireNonNull(decompressed);
        compressed.retain();
        decompressed.retain();
        try {
            OutputStream out = createOutputStream(new ByteBufOutputStream(compressed));
            byte[] buffer = new byte[4096];
            while (decompressed.isReadable()) {
                int numRead = Math.min(buffer.length, decompressed.readableBytes());
                decompressed.readBytes(buffer, 0, numRead);
                out.write(buffer, 0, numRead);
            }
        } finally {
            compressed.release();
            decompressed.release();
        }
    }


    public InputStream createInputStream(InputStream in) throws IOException {
        switch (this) {
            case LZ4_BLOCK:
                return new LZ4BlockInputStream(in);
            case LZ4_FRAMED:
                // Use commons compress, since they support dependent blocks
                return new FramedLZ4CompressorInputStream(in);
            case GZIP:
                return new GZIPInputStream(in);
            default:
                throw new AssertionError(this);
        }
    }
    public OutputStream createOutputStream(OutputStream out) throws IOException {
        switch (this) {
            case LZ4_BLOCK:
                return new LZ4BlockOutputStream(out);
            case LZ4_FRAMED:
                return new LZ4FrameOutputStream(out);
            case GZIP:
                return new GZIPOutputStream(out);
            default:
                throw new AssertionError(this);
        }
    }
}
