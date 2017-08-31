package net.techcable.supersrg.utils;

import java.util.Objects;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentMap;

import javax.annotation.Nonnull;

import com.google.common.cache.Cache;
import com.google.common.cache.CacheBuilder;
import com.google.common.cache.CacheLoader;
import com.google.common.cache.LoadingCache;

import org.objectweb.asm.commons.Remapper;

public class ClassMappingsRemapper extends Remapper {
    private final FastMappings mappings;
    private final LoadingCache<String, String> signatureCache = CacheBuilder.newBuilder().build(new CacheLoader<String, String>() {
        @Override
        public String load(@Nonnull String key) throws Exception {
            return ClassMappingsRemapper.super.mapMethodDesc(key);
        }
    });
    private final LoadingCache<String, String> typeCache = CacheBuilder.newBuilder().build(new CacheLoader<String, String>() {
        @Override
        public String load(@Nonnull String key) throws Exception {
            return ClassMappingsRemapper.super.mapDesc(key);
        }
    });
    public ClassMappingsRemapper(FastMappings mappings) {
        this.mappings = Objects.requireNonNull(mappings);
    }

    @Override
    public String mapDesc(String desc) {
        return typeCache.getUnchecked(desc);
    }

    @Override
    public String mapType(String type) {
        String newName = this.map(type);
        return newName != null ? newName : type;
    }

    @Override
    public String mapMethodDesc(String desc) {
        return signatureCache.getUnchecked(desc);
    }

    @Override
    public String mapFieldName(String owner, String name, String desc) {
        FastMappings.ClassMappings classMappings = mappings.getClassMappings(owner);
        String fieldName;
        if (classMappings != null) {
            fieldName = classMappings.getFieldName(name);
            return fieldName != null ? fieldName : name;
        }
        return name;
    }

    @Override
    public String mapMethodName(String owner, String name, String desc) {
        FastMappings.ClassMappings classMappings = mappings.getClassMappings(owner);
        String methodName;
        if (classMappings != null) {
            methodName = classMappings.getMethodName(name, desc);
            return methodName != null ? methodName : name;
        }
        return name;
    }

    @Override
    public String map(String typeName) {
        FastMappings.ClassMappings classMappings = mappings.getClassMappings(typeName);
        return classMappings != null ? classMappings.getRemappedName() : null;
    }

}
