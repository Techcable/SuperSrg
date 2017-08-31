package net.techcable.supersrg.utils;

import lombok.*;

import java.io.File;
import java.io.IOException;
import java.util.Arrays;
import java.util.HashMap;
import java.util.Map;
import java.util.Objects;
import java.util.Optional;
import javax.annotation.Nonnull;
import javax.annotation.Nullable;

import com.google.common.cache.CacheBuilder;
import com.google.common.cache.CacheLoader;
import com.google.common.cache.LoadingCache;
import com.google.common.collect.HashBasedTable;
import com.google.common.collect.ImmutableMap;
import com.google.common.collect.ImmutableTable;
import com.google.common.collect.Table;
import com.google.common.collect.Tables;

import net.techcable.srglib.FieldData;
import net.techcable.srglib.JavaType;
import net.techcable.srglib.MethodData;
import net.techcable.srglib.MethodSignature;
import net.techcable.srglib.format.MappingsFormat;
import net.techcable.srglib.mappings.ImmutableMappings;
import net.techcable.srglib.mappings.MutableMappings;
import net.techcable.supersrg.binary.BinaryMappingsDecoder;
import net.techcable.supersrg.binary.BinaryMappingsException;
import net.techcable.supersrg.cmd.CommandException;

import org.objectweb.asm.Type;

/**
 * A faster alternative to {@link net.techcable.srglib.mappings.ImmutableMappings}.
 */
public class FastMappings {
    private final ImmutableMap<String, ClassMappings> classes;
    private final LoadingCache<String, Optional<String>> typeDescriptorCache;
    private final LoadingCache<String, Optional<String>> methodDescriptorCache;

    public FastMappings(ImmutableMap<String, ClassMappings> classes) {
        this.classes = Objects.requireNonNull(classes);
        this.typeDescriptorCache = CacheBuilder.newBuilder()
                .softValues()
                .maximumSize(10_000)
                .build(new CacheLoader<String, Optional<String>>() {
                    @Override
                    public Optional<String> load(@Nonnull String key) throws Exception {
                        return Optional.ofNullable(FastMappings.this.remapType(Type.getType(key)));
                    }
                });
        this.methodDescriptorCache = CacheBuilder.newBuilder()
                .softValues()
                .maximumSize(100_000)
                .build(new CacheLoader<String, Optional<String>>() {
                    @Override
                    public Optional<String> load(@Nonnull String key) throws Exception {
                        return Optional.ofNullable(FastMappings.this.remapType(Type.getMethodType(key)));
                    }
                });
    }

    @Nullable
    public ClassMappings getClassMappings(String className) {
        return classes.get(className);
    }

    @Nullable
    public String remapTypeDescriptor(String descriptor) {
        return typeDescriptorCache.getUnchecked(descriptor).orElse(null);
    }
    public String remapMethodDescriptor(String descriptor) {
        return methodDescriptorCache.getUnchecked(descriptor).orElse(null);
    }
    @Nullable
    private String remapType(Type original) {
        switch (original.getSort()) {
            case Type.METHOD: {
                Type[] originalParameterTypes = original.getArgumentTypes();
                StringBuilder result = new StringBuilder(64);
                result.append('(');
                boolean needsRemap = false;
                for (Type originalType : originalParameterTypes) {
                    String remappedType = remapType(originalType);
                    if (remappedType != null) {
                        needsRemap = true;
                        result.append(remappedType);
                    } else {
                        result.append(originalType.getDescriptor());
                    }
                }
                result.append(')');
                Type originalReturnType = original.getReturnType();
                String remappedReturnType = remapType(originalReturnType);
                if (remappedReturnType != null) {
                    needsRemap = true;
                    result.append(remappedReturnType);
                } else {
                    result.append(originalReturnType.getDescriptor());
                }
                return needsRemap ? result.toString() : null;
            }
            case Type.ARRAY:
                Type elementType = original.getElementType();
                if (elementType.getSort() == Type.OBJECT) {
                    ClassMappings classMappings = classes.get(elementType.getInternalName());
                    String remappedName;
                    if (classMappings != null && (remappedName = classMappings.remappedName) != null) {
                        int dimensions = original.getDimensions();
                        char[] result = new char[remappedName.length() + 2 + dimensions];
                        Arrays.fill(result, 0, dimensions, '[');
                        result[dimensions] = 'L';
                        remappedName.getChars(0, remappedName.length(), result, dimensions + 1);
                        result[dimensions + remappedName.length() + 1] = ';';
                        return String.valueOf(result);
                    }
                }
                return null;
            case Type.OBJECT:
                ClassMappings classMappings = classes.get(original.getInternalName());
                String remappedName;
                if (classMappings != null && (remappedName = classMappings.remappedName) != null) {
                    char[] result = new char[2 + remappedName.length()];
                    result[0] = 'L';
                    remappedName.getChars(0, remappedName.length(), result, 1);
                    result[remappedName.length() + 1] = ';';
                    return String.valueOf(result);
                }
                return null;
            default:
                return null;
        }
    }

    public static class ClassMappings {
        @Getter
        private final String originalName;
        @Nullable
        @Getter
        private final String remappedName;
        private final ImmutableMap<String, String> fieldNames;
        // NOTE: Indexed by (descriptor, name) to reduce the size of the row map
        private final ImmutableTable<String, String, String> methodNames;

        public ClassMappings(
                String originalName,
                @Nullable String remappedName,
                ImmutableMap<String, String> fieldNames,
                ImmutableTable<String, String, String> methodNames
        ) {
            this.originalName = Objects.requireNonNull(originalName);
            this.remappedName = remappedName;
            this.fieldNames = Objects.requireNonNull(fieldNames);
            this.methodNames = ImmutableTable.copyOf(Tables.transpose(methodNames));
        }

        @Nullable
        public String getFieldName(String originalName) {
            return fieldNames.get(originalName);
        }
        @Nullable
        public String getMethodName(String originalName, String originalDescriptor) {
            return methodNames.get(originalDescriptor, originalName);
        }
    }

    private ImmutableMappings slow;
    public ImmutableMappings asSlow() {
        ImmutableMappings result = this.slow;
        if (result == null) {
            this.slow = result = this.createSlow();
        }
        return result;
    }
    private ImmutableMappings createSlow() {
        MutableMappings mappings = MutableMappings.create();
        for (ClassMappings classMappings : classes.values()) {
            JavaType originalType = JavaType.fromInternalName(classMappings.getOriginalName());
            if (classMappings.remappedName != null) {
                JavaType remappedType = JavaType.fromInternalName(classMappings.remappedName);
                mappings.putClass(originalType, remappedType);
            }
            classMappings.fieldNames.forEach((originalName, remappedName) -> {
                FieldData original = FieldData.create(originalType, originalName);
                mappings.putField(original, remappedName);
            });
            classMappings.methodNames.rowMap().forEach((originalName, descriptorMap) -> descriptorMap.forEach((originalDescriptor, remappedName) -> {
                MethodData original = MethodData.create(originalType, originalName, MethodSignature.fromDescriptor(originalDescriptor));
                mappings.putMethod(original, remappedName);
            }));
        }
        return mappings.snapshot();
    }
    public static FastMappings fromSlow(ImmutableMappings slow) {
        Map<String, String> classNames = new HashMap<>();
        Map<String, Table<String, String, String>> methods = new HashMap<>();
        Map<String, Map<String, String>> fields = new HashMap<>();
        slow.forEachClass((original, renamed) -> classNames.put(original.getInternalName(), !original.equals(renamed) ? renamed.getInternalName() : null));
        slow.forEachMethod((original, renamed) -> {
            String originalClass = original.getDeclaringType().getInternalName();
            classNames.putIfAbsent(originalClass, null);
            Table<String, String, String> methodTable = methods.computeIfAbsent(originalClass, (k) -> HashBasedTable.create());
            if (!original.getName().equals(renamed.getName())) {
                methodTable.put(original.getName(), original.getSignature().getDescriptor(), renamed.getName());
            }
        });
        slow.forEachField((original, renamed) -> {
            String originalClass = original.getDeclaringType().getInternalName();
            Map<String, String> fieldMap = fields.computeIfAbsent(originalClass, (k) -> new HashMap<>());
            classNames.putIfAbsent(originalClass, null);
            if (!original.getName().equals(renamed.getName())) {
                fieldMap.put(original.getName(), renamed.getName());
            }
        });
        ImmutableMap.Builder<String, ClassMappings> classMappings = ImmutableMap.builder();
        classNames.forEach((originalClass, renamedClass) -> {
            Table<String, String, String> methodTable = methods.getOrDefault(originalClass, ImmutableTable.of());
            Map<String, String> fieldMap = fields.getOrDefault(originalClass, ImmutableMap.of());
            classMappings.put(originalClass, new ClassMappings(
                    originalClass,
                    renamedClass,
                    ImmutableMap.copyOf(fieldMap),
                    ImmutableTable.copyOf(methodTable)
            ));
        });
        FastMappings result = new FastMappings(classMappings.build());
        result.slow = slow;
        return result;
    }
    public static FastMappings fromFile(File mappingsFile) throws IOException, BinaryMappingsException {
        int dotIndex = mappingsFile.getPath().indexOf('.');
        String extension = dotIndex >= 0 ? mappingsFile.getPath().substring(dotIndex + 1) : "";
        switch (extension) {
            case "srg":
                return FastMappings.fromSlow(MappingsFormat.SEARGE_FORMAT.parseFile(mappingsFile).snapshot());
            case "srg.dat":
                return BinaryMappingsDecoder.parseFile(mappingsFile);
            default:
                throw new CommandException("Unknown mapping file extension: " + extension);
        }
    }
}
