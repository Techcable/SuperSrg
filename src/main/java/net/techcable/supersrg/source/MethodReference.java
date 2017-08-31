package net.techcable.supersrg.source;

import lombok.*;

import java.util.Objects;
import java.util.Random;

import javax.annotation.Nullable;

import com.google.common.base.Preconditions;

import io.netty.buffer.ByteBuf;

import net.techcable.srglib.MethodData;
import net.techcable.srglib.MethodSignature;
import net.techcable.srglib.mappings.Mappings;
import net.techcable.supersrg.utils.FastMappings;
import net.techcable.supersrg.utils.RandomUtils;
import net.techcable.supersrg.utils.SerializationUtils;

@Getter
@EqualsAndHashCode(callSuper = false)
public class MethodReference extends MemberReference {
    private final MethodData referencedMethod;

    public MethodReference(FileLocation location, MethodData referencedMethod) {
        super(location);
        this.referencedMethod = Objects.requireNonNull(referencedMethod);
        Preconditions.checkArgument(
                location.size() == this.getName().length(),
                "Method is size %s, but has name %s @",
                location.size(),
                this.getName(),
                location
        );
    }

    @Override
    public String getName() {
        return referencedMethod.getName();
    }

    @Override
    public MethodReference remap(Mappings mappings) {
        MethodData newMethod = mappings.getNewMethod(referencedMethod);
        return new MethodReference(
                new FileLocation(this.getStart(), this.getStart() + newMethod.getName().length()),
                newMethod
        );
    }

    @Override
    @Nullable
    public String remapName(FastMappings mappings) {
        FastMappings.ClassMappings classMappings = mappings.getClassMappings(this.getReferencedMethod().getDeclaringType().getInternalName());
        if (classMappings != null) {
            return classMappings.getMethodName(this.getName(), referencedMethod.getSignature().getDescriptor());
        }
        return null;
    }

    public void serialize(ByteBuf output) {
        getLocation().serialize(output);
        SerializationUtils.writePrefixedString(output, referencedMethod.getInternalName());
        SerializationUtils.writePrefixedString(output, referencedMethod.getSignature().getDescriptor());
    }

    public static MethodReference deserialize(ByteBuf input) {
        FileLocation location = FileLocation.deserialize(input);
        String internalName = SerializationUtils.readPrefixedString(input);
        String descriptor = SerializationUtils.readPrefixedString(input);
        return new MethodReference(
                location,
                MethodData.fromInternalName(internalName, MethodSignature.fromDescriptor(descriptor))
        );
    }
    public static MethodReference createRandom(Random random) {
        MethodData referencedMethod = RandomUtils.randomMethod(random);
        int start = random.nextInt(1000);
        int end = start + referencedMethod.getName().length();
        return new MethodReference(
                new FileLocation(start, end),
                referencedMethod
        );
    }

    @Override
    public String toString() {
        return referencedMethod.getInternalName() + referencedMethod.getSignature().getDescriptor() + "@" + getLocation();
    }
}
