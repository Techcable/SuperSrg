package net.techcable.supersrg.source;

import com.google.common.collect.ImmutableListMultimap;
import com.google.common.collect.ImmutableMap;
import com.google.common.collect.ImmutableMultimap;

public class RangeMapBuilder {
    private ImmutableListMultimap.Builder<String, MethodReference> methodReferences = ImmutableListMultimap.builder();
    private ImmutableListMultimap.Builder<String, FieldReference> fieldReferences = ImmutableListMultimap.builder();
    public void addMethodReference(String fileName, MethodReference ref) {
        methodReferences.put(fileName, ref);
    }
    public void addFieldReference(String fileName, FieldReference ref) {
        fieldReferences.put(fileName, ref);
    }
    public RangeMap build() {
        return RangeMap.create(fieldReferences.build(), methodReferences.build(), ImmutableMap.of());
    }
}
