package net.techcable.supersrg.source;

import lombok.*;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.StringWriter;
import java.io.UncheckedIOException;
import java.util.Arrays;
import java.util.Collection;
import java.util.HashMap;
import java.util.Map;
import java.util.Objects;
import java.util.Random;
import java.util.Set;

import com.google.common.base.Verify;
import com.google.common.collect.ArrayListMultimap;
import com.google.common.collect.ImmutableList;
import com.google.common.collect.ImmutableListMultimap;
import com.google.common.collect.ImmutableMap;
import com.google.common.collect.ImmutableSet;
import com.google.common.collect.ListMultimap;
import com.google.common.hash.Hasher;
import com.google.common.hash.Hashing;
import com.google.gson.stream.JsonWriter;

import io.netty.buffer.ByteBuf;
import io.netty.buffer.ByteBufUtil;
import io.netty.buffer.Unpooled;

import net.techcable.supersrg.utils.ArrayUtils;
import net.techcable.supersrg.utils.RandomUtils;

import org.msgpack.core.MessageBufferPacker;
import org.msgpack.core.MessagePack;
import org.msgpack.core.MessagePacker;
import org.msgpack.core.MessageUnpacker;

import static org.msgpack.core.Preconditions.*;

public class RangeMap {
    @Getter
    private final ImmutableListMultimap<String, FieldReference> fieldReferences;
    @Getter
    private final ImmutableListMultimap<String, MethodReference> methodReferences;
    private final ImmutableMap<String, byte[]> fileHashes;

    private RangeMap(
            ImmutableListMultimap<String, FieldReference> fieldReferences,
            ImmutableListMultimap<String, MethodReference> methodReferences,
            ImmutableMap<String, byte[]> fileHashes
    ) {
        this.fieldReferences = Objects.requireNonNull(fieldReferences);
        this.methodReferences = Objects.requireNonNull(methodReferences);
        this.fileHashes = Objects.requireNonNull(fileHashes);
    }
    public ImmutableList<FieldReference> getFieldReferences(String fileName) {
        return fieldReferences.get(fileName);
    }
    public ImmutableList<MethodReference> getMethodReferences(String fileName) {
        return methodReferences.get(fileName);
    }
    public ImmutableList<MemberReference> allSortedReferences(String fileName) {
        MemberReference[] array = ArrayUtils.joinImmutableLists(MemberReference.class, this.getFieldReferences(fileName), this.getMethodReferences(fileName));
        Arrays.sort(array);
        return ImmutableList.copyOf(array);
    }
    public byte[] getFileHash(String fileName) {
        return fileHashes.get(fileName);
    }
    public boolean hasFileHash(String fileName, byte[] expected) {
        Objects.requireNonNull(expected, "Null expected hash");
        return Arrays.equals(this.getFileHash(fileName), expected);
    }
    public ImmutableMap<String, byte[]> getFileHashes() {
        return ImmutableMap.copyOf(this.fileHashes);
    }
    public void save(File target) throws IOException{
        try (MessagePacker packer = MessagePack.newDefaultPacker(new FileOutputStream(target))) {
            this.pack(packer);
        }
    }
    public RangeMap update(RangeMap other) {
        ListMultimap<String, FieldReference> fieldReferences = ArrayListMultimap.create(this.fieldReferences);
        ListMultimap<String, MethodReference> methodReferences = ArrayListMultimap.create(this.methodReferences);
        Map<String, byte[]> fileHashes = new HashMap<>(this.fileHashes);
        other.fieldReferences.asMap().forEach(fieldReferences::replaceValues);
        other.methodReferences.asMap().forEach(methodReferences::replaceValues);
        other.fileHashes.forEach(fileHashes::put);
        return RangeMap.copyOf(fieldReferences, methodReferences, fileHashes);
    }
    public static RangeMap load(File target) throws IOException {
        try (MessageUnpacker unpacker = MessagePack.newDefaultUnpacker(new FileInputStream(target))) {
            return RangeMap.unpack(unpacker);
        }
    }
    @SneakyThrows(IOException.class)
    public ByteBuf serialize() {
        MessageBufferPacker packer = MessagePack.newDefaultBufferPacker();
        this.pack(packer);
        return Unpooled.wrappedBuffer(packer.toByteArray());
    }
    public void pack(MessagePacker packer) throws IOException {
        ImmutableMap<String, byte[]> fileHashes = ImmutableMap.copyOf(this.fileHashes);
        packer.packMapHeader(3);
        packer.packString("fieldReferences");
        ImmutableMap<String, Collection<FieldReference>> fieldReferences = this.fieldReferences.asMap();
        ImmutableMap<String, Collection<MethodReference>> methodReferences = this.methodReferences.asMap();
        packer.packMapHeader(fieldReferences.size());
        ByteBuf buffer = Unpooled.buffer(256);
        for (Map.Entry<String, Collection<FieldReference>> entry : fieldReferences.entrySet()) {
            packer.packString(entry.getKey());
            ImmutableList<FieldReference> references = ImmutableList.copyOf(entry.getValue());
            packer.packArrayHeader(references.size());
            for (int i = 0; i < references.size(); i++) {
                FieldReference reference = references.get(i);
                buffer.writerIndex(0);
                reference.serialize(buffer);
                buffer.readerIndex(0);
                int size = buffer.readableBytes();
                packer.packBinaryHeader(size);
                packer.writePayload(buffer.array(), buffer.arrayOffset(), size);
            }
        }
        packer.packString("methodReferences");
        packer.packMapHeader(methodReferences.size());
        for (Map.Entry<String, Collection<MethodReference>> entry : methodReferences.entrySet()) {
            packer.packString(entry.getKey());
            ImmutableList<MethodReference> references = ImmutableList.copyOf(entry.getValue());
            packer.packArrayHeader(references.size());
            for (int i = 0; i < references.size(); i++) {
                MethodReference reference = references.get(i);
                buffer.writerIndex(0);
                reference.serialize(buffer);
                buffer.readerIndex(0);
                int size = buffer.readableBytes();
                packer.packBinaryHeader(size);
                packer.writePayload(buffer.array(), buffer.arrayOffset(), size);
            }
        }
        packer.packString("fileHashes");
        packer.packMapHeader(fileHashes.size());
        for (Map.Entry<String, byte[]> fileHash : fileHashes.entrySet()) {
            packer.packString(fileHash.getKey());
            byte[] hash = fileHash.getValue();
            packer.packBinaryHeader(hash.length);
            packer.writePayload(hash);
        }
    }
    private static final RangeMap EMPTY = new RangeMap(ImmutableListMultimap.of(), ImmutableListMultimap.of(), ImmutableMap.of());
    public static RangeMap empty() {
        return EMPTY;
    }
    public static RangeMap copyOf(
            ListMultimap<String, FieldReference> fieldReferences,
            ListMultimap<String, MethodReference> methodReferences,
            Map<String, byte[]> fileHashes
    ) {
        if (fieldReferences.isEmpty() && methodReferences.isEmpty() && fileHashes.isEmpty()) {
            return EMPTY;
        }
        return create(
                ImmutableListMultimap.copyOf(fieldReferences),
                ImmutableListMultimap.copyOf(methodReferences),
                ImmutableMap.copyOf(fileHashes)
        );
    }
    public static RangeMap create(
            ImmutableListMultimap<String, FieldReference> fieldReferences,
            ImmutableListMultimap<String, MethodReference> methodReferences,
            ImmutableMap<String, byte[]> fileHashes
    ) {
        if (fieldReferences.isEmpty() && methodReferences.isEmpty() && fileHashes.isEmpty()) {
            return EMPTY;
        } else {
            return new RangeMap(fieldReferences, methodReferences, fileHashes);
        }
    }
    @SneakyThrows(IOException.class)
    public static RangeMap deserialize(ByteBuf buffer) {
        final MessageUnpacker unpacker;
        if (buffer.hasArray()) {
            unpacker = MessagePack.newDefaultUnpacker(
                    buffer.array(),
                    buffer.arrayOffset() + buffer.readerIndex(),
                    buffer.readableBytes()
            );
        } else {
            byte[] array = new byte[buffer.readableBytes()];
            buffer.getBytes(buffer.readerIndex(), array);
            unpacker = MessagePack.newDefaultUnpacker(array);
        }
        RangeMap result = RangeMap.unpack(unpacker);
        buffer.readerIndex(Math.toIntExact(buffer.readerIndex() + unpacker.getTotalReadBytes()));
        return result;
    }
    public static RangeMap unpack(MessageUnpacker unpacker) throws IOException {
        int objectSize = unpacker.unpackMapHeader();
        ImmutableListMultimap<String, FieldReference> fieldReferences = null;
        ImmutableListMultimap<String, MethodReference> methodReferences = null;
        ImmutableMap<String, byte[]> fileHashes = null;
        ByteBuf buffer = Unpooled.buffer(256);
        Verify.verify(buffer.hasArray());
        for (int fieldNum = 0; fieldNum < objectSize; fieldNum++) {
            String key = unpacker.unpackString();
            switch (key) {
                case "fieldReferences": {
                    checkState(fieldReferences == null, "Already got fieldReferences");
                    int numEntries = unpacker.unpackMapHeader();
                    ImmutableListMultimap.Builder<String, FieldReference> builder = ImmutableListMultimap.builder();
                    for (int entryNum = 0; entryNum < numEntries; entryNum++) {
                        String fileName = unpacker.unpackString();
                        int numReferences = unpacker.unpackArrayHeader();
                        FieldReference[] references = new FieldReference[numReferences];
                        for (int i = 0; i < numReferences; i++) {
                            int serializedSize = unpacker.unpackBinaryHeader();
                            buffer.readerIndex(0);
                            buffer.writerIndex(0);
                            buffer.ensureWritable(serializedSize);
                            unpacker.readPayload(buffer.array(), buffer.arrayOffset(), serializedSize);
                            buffer.writerIndex(serializedSize);
                            Verify.verify(buffer.readerIndex() == 0);
                            FieldReference reference = FieldReference.deserialize(buffer);
                            references[i] = reference;
                        }
                        builder.putAll(fileName, ImmutableList.copyOf(references));
                    }
                    fieldReferences = builder.build();
                    break;
                }
                case "methodReferences": {
                    checkState(methodReferences == null, "Already got methodReferences");
                    int numEntries = unpacker.unpackMapHeader();
                    ImmutableListMultimap.Builder<String, MethodReference> builder = ImmutableListMultimap.builder();
                    for (int entryNum = 0; entryNum < numEntries; entryNum++) {
                        String fileName = unpacker.unpackString();
                        int numReferences = unpacker.unpackArrayHeader();
                        MethodReference[] references = new MethodReference[numReferences];
                        for (int i = 0; i < numReferences; i++) {
                            int serializedSize = unpacker.unpackBinaryHeader();
                            buffer.readerIndex(0);
                            buffer.writerIndex(0);
                            buffer.ensureWritable(serializedSize);
                            unpacker.readPayload(buffer.array(), buffer.arrayOffset(), serializedSize);
                            buffer.writerIndex(serializedSize);
                            Verify.verify(buffer.readerIndex() == 0);
                            MethodReference reference = MethodReference.deserialize(buffer);
                            references[i] = reference;
                        }
                        builder.putAll(fileName, ImmutableList.copyOf(references));
                    }
                    methodReferences = builder.build();
                    break;
                }
                case "fileHashes": {
                    checkState(fileHashes == null, "Already got fileHashes");
                    int numEntries = unpacker.unpackMapHeader();
                    ImmutableMap.Builder<String, byte[]> builder = ImmutableMap.builder();
                    for (int entryNum = 0; entryNum < numEntries; entryNum++) {
                        String fileName = unpacker.unpackString();
                        byte[] hash = unpacker.readPayload(unpacker.unpackBinaryHeader());
                        builder.put(fileName, hash);
                    }
                    fileHashes = builder.build();
                    break;
                }
            }
        }
        checkState(fieldReferences != null, "No fieldReferences");
        checkState(methodReferences != null, "No methodReferences");
        checkState(fileHashes != null, "No fileHashes");
        return new RangeMap(fieldReferences, methodReferences, fileHashes);
    }
    public static RangeMap createRandom(Random random) {
        int numFiles = random.nextInt(5);
        ImmutableListMultimap.Builder<String, FieldReference> fieldReferences = ImmutableListMultimap.builder();
        ImmutableListMultimap.Builder<String, MethodReference> methodReferences = ImmutableListMultimap.builder();
        ImmutableMap.Builder<String, byte[]> fileHashes = ImmutableMap.builder();
        for (int fileIndex = 0; fileIndex < numFiles; fileIndex++) {
            int numFields = random.nextInt(15);
            int numMethods = random.nextInt(15);
            String fileName = RandomUtils.randomClassName(random).replace('.', '/') + ".java";
            byte[] hash = new byte[32];
            random.nextBytes(hash);
            fileHashes.put(fileName, hash);
            for (int i = 0; i < numFields; i++) {
                fieldReferences.put(
                        fileName,
                        FieldReference.createRandom(random)
                );
            }
            for (int i = 0; i < numMethods; i++) {
                methodReferences.put(
                        fileName,
                        MethodReference.createRandom(random)
                );
            }
        }
        return new RangeMap(fieldReferences.build(), methodReferences.build(), fileHashes.build());
    }

    public void toJson(JsonWriter writer) throws IOException {
        try {
            writer.beginObject();
            writer.name("fileHashes");
            writer.beginObject();
            this.fileHashes.forEach((fileName, hash) -> {
                try {
                    writer.name(fileName);
                    writer.value(ByteBufUtil.hexDump(hash));
                } catch (IOException e) {
                    throw new UncheckedIOException(e);
                }
            });
            writer.endObject();
            writer.name("fieldReferences");
            writer.beginObject();
            this.fieldReferences.asMap().forEach((fileName, references) -> {
                try {
                    writer.name(fileName);
                    writer.beginArray();
                    for (FieldReference reference : references) {
                        writer.value(reference.toString());
                    }
                    writer.endArray();
                } catch (IOException e) {
                    throw new UncheckedIOException(e);
                }
            });
            writer.endObject();
            writer.name("methodReferences");
            writer.beginObject();
            this.methodReferences.asMap().forEach((fileName, references) -> {
                try {
                    writer.name(fileName);
                    writer.beginArray();
                    for (MethodReference reference : references) {
                        writer.value(reference.toString());
                    }
                    writer.endArray();
                } catch (IOException e) {
                    throw new UncheckedIOException(e);
                }
            });
            writer.endObject();
            writer.endObject();
        } catch (UncheckedIOException e) {
            throw e.getCause();
        }
    }

    private ImmutableSet<String> knownFiles;
    public ImmutableSet<String> knownFiles() {
        ImmutableSet<String> knownFiles = this.knownFiles;
        if (knownFiles == null) {
            knownFiles = this.knownFiles = createKnownFiles();
        }
        return knownFiles;
    }
    private ImmutableSet<String> createKnownFiles() {
        return ImmutableSet.copyOf(ArrayUtils.joinImmutableLists(
                String.class,
                this.fieldReferences.keySet().asList(),
                this.methodReferences.keySet().asList()
        ));
    }

    @Override
    @SneakyThrows(IOException.class)
    public String toString() {
        StringWriter writer = new StringWriter();
        JsonWriter jsonWriter = new JsonWriter(writer);
        jsonWriter.setIndent("  ");
        this.toJson(jsonWriter);
        return writer.toString();
    }

    @Override
    public boolean equals(Object obj) {
        if (obj == this) return true;
        if (obj == null) return false;
        if (obj instanceof RangeMap) {
            RangeMap other = (RangeMap) obj;
            // We have to hand-code the equality check for the fileHashes since equals doesn't work on arrays
            ImmutableMap<String, byte[]> fileHashes = ImmutableMap.copyOf(this.fileHashes);
            ImmutableMap<String, byte[]> otherFileHashes = ImmutableMap.copyOf(other.fileHashes);
            if (fileHashes.size() != otherFileHashes.size()) {
                return false;
            }
            for (Map.Entry<String, byte[]> entry : fileHashes.entrySet()) {
                String fileName = entry.getKey();
                byte[] hash = entry.getValue();
                byte[] otherHash = otherFileHashes.get(fileName);
                if (!Arrays.equals(hash, otherHash)) {
                    return false;
                }
            }
            // NOTE: Must sort the references
            ImmutableSet<String> knownFiles = this.knownFiles();
            if (!knownFiles.equals(other.knownFiles())) return false;
            for (String knownFile : knownFiles) {
                if (!this.allSortedReferences(knownFile).equals(other.allSortedReferences(knownFile))) {
                    return false;
                }
            }
            return true;
        }
        return false;
    }

    private int hashCode;
    @Override
    public int hashCode() {
        int hashCode = this.hashCode;
        if (hashCode == 0) {
            hashCode = this.computeHashCode();
            if (hashCode == 0) hashCode = 1;
            this.hashCode = hashCode;
        }
        return hashCode;
    }

    private int computeHashCode() {
        Hasher hasher = Hashing.goodFastHash(32).newHasher();
        fileHashes.forEach((fileName, hash) -> {
            hasher.putUnencodedChars(fileName);
            hasher.putBytes(hash);
        });
        knownFiles().forEach(fileName -> {
            hasher.putUnencodedChars(fileName);
            allSortedReferences(fileName).forEach((ref) -> hasher.putInt(ref.hashCode()));
        });
        return hasher.hash().asInt();
    }
}
