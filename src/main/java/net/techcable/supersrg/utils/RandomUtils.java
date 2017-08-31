package net.techcable.supersrg.utils;

import java.util.Random;

import com.google.common.base.Preconditions;
import com.google.common.base.Verify;
import com.google.common.collect.ImmutableList;

import net.techcable.srglib.FieldData;
import net.techcable.srglib.JavaType;
import net.techcable.srglib.MethodData;
import net.techcable.srglib.MethodSignature;
import net.techcable.srglib.PrimitiveType;

public class RandomUtils {
    private RandomUtils() {}

    public static final Random RANDOM = new Random();

    public static char randomChar(Random random) {
        int value = random.nextInt(26 * 2 + 10);
        if (value < 10) {
            return (char) ('0' + value);
        } else if (value < 36) {
            return (char) ('a' + (value - 10));
        } else if (value < 62){
            return (char) ('A' + (value - 36));
        } else {
            throw new AssertionError(value);
        }
    }
    public static String randomString(Random random, int length) {
        Preconditions.checkArgument(length >= 0, "Invalid length: %s", length);
        char[] result = new char[length];
        for (int i = 0; i < length; i++) {
            result[i] = randomChar(random);
        }
        return String.valueOf(result);
    }
    public static String randomClassName(Random random) {
        int packageParts = random.nextInt(3) + 1;
        StringBuilder result = new StringBuilder(packageParts * 6 + 5);
        for (int i = 0; i < packageParts; i++) {
            result.append(randomName(random));
            result.append('.');
        }
        Verify.verify(result.charAt(result.length() - 1) == '.');
        result.append(randomName(random));
        return result.toString();
    }
    public static String randomName(Random random) {
        int nameLength = random.nextInt(2) + 4;
        char[] result = new char[nameLength];
        result[0] = (char) ('A' + random.nextInt(26));
        for (int i = 1; i < nameLength; i++) {
            result[i] = randomChar(random);
        }
        return String.valueOf(result);
    }
    public static JavaType randomClass(Random random) {
        return JavaType.fromName(randomClassName(random));
    }
    public static FieldData randomField(Random random) {
        return FieldData.create(
                randomClass(random),
                randomName(random)
        );
    }
    public static MethodData randomMethod(Random random) {
        JavaType[] parameterTypes = new JavaType[random.nextInt(3)];
        for (int i = 0; i < parameterTypes.length; i++) {
            parameterTypes[i] = randomBasicType(random);
        }
        JavaType returnType = random.nextBoolean() ? PrimitiveType.VOID : randomBasicType(random);
        return MethodData.create(
                randomClass(random),
                randomName(random),
                MethodSignature.create(
                        ImmutableList.copyOf(parameterTypes),
                        returnType
                )
        );
    }
    private static final JavaType STRING_TYPE = JavaType.fromName("java.lang.String");
    private static final JavaType LIST_TYPE = JavaType.fromName("java.util.List");
    private static final JavaType INT_ARRAY_TYPE = JavaType.createArray(1, PrimitiveType.INT);
    private static final JavaType MAP_TYPE = JavaType.fromName("java.util.Map");
    private static final PrimitiveType[] NONVOID_PRIMITIVES;
    static {
        PrimitiveType[] nonvoid = new PrimitiveType[PrimitiveType.values().length - 1];
        int resultSize = 0;
        for (PrimitiveType primitiveType : PrimitiveType.values()) {
            if (primitiveType != PrimitiveType.VOID) {
                nonvoid[resultSize++] = primitiveType;
            }
        }
        Verify.verify(resultSize == nonvoid.length);
        NONVOID_PRIMITIVES = nonvoid;
    }
    private static final JavaType[] BASIC_OBJECT_TYPES = new JavaType[] {
            STRING_TYPE,
            LIST_TYPE,
            INT_ARRAY_TYPE,
            MAP_TYPE
    };
    public static JavaType randomBasicType(Random random) {
        int id = random.nextInt(NONVOID_PRIMITIVES.length + BASIC_OBJECT_TYPES.length);
        if (id < NONVOID_PRIMITIVES.length) {
            return NONVOID_PRIMITIVES[id];
        } else {
            return BASIC_OBJECT_TYPES[id - NONVOID_PRIMITIVES.length];
        }
    }
}
