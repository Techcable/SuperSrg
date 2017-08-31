package net.techcable.supersrg.utils;

import com.google.common.base.Verify;
import com.google.common.collect.ImmutableList;
import com.google.common.collect.ObjectArrays;

public class ArrayUtils {
    private ArrayUtils() {}
    public static <T> T[] joinImmutableLists(Class<T> elementType, ImmutableList<? extends T> first, ImmutableList<? extends T> second) {
        int firstSize = first.size();
        int secondSize = second.size();
        int expectedSize = firstSize + secondSize;
        T[] result = ObjectArrays.newArray(elementType, expectedSize);
        int resultSize = 0;
        for (int i = 0; i < firstSize; i++) {
            result[resultSize++] = first.get(i);
        }
        for (int i = 0; i < secondSize; i++) {
            result[resultSize++] = second.get(i);
        }
        Verify.verify(resultSize == result.length && resultSize == expectedSize);
        return result;
    }
    public static final int[] EMPTY_INT_ARRAY = new int[0];
    public static final byte[] EMPTY_BYTE_ARRAY = new byte[0];
    public static final long[] EMPTY_LONG_ARRAY = new long[0];
    public static final Object[] EMPTY_OBJECT_ARRAY = new Object[0];
}
