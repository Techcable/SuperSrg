package net.techcable.supersrg.source;

import jdk.internal.dynalink.support.ClassMap;
import lombok.*;

import java.util.Objects;
import java.util.Random;

import javax.annotation.Nullable;

import com.google.common.base.Preconditions;
import com.google.gson.JsonPrimitive;

import io.netty.buffer.ByteBuf;
import io.netty.buffer.ByteBufUtil;

import net.techcable.srglib.FieldData;
import net.techcable.srglib.mappings.Mappings;
import net.techcable.supersrg.utils.FastMappings;
import net.techcable.supersrg.utils.RandomUtils;
import net.techcable.supersrg.utils.SerializationUtils;

@Getter
@EqualsAndHashCode(callSuper = false)
public class FieldReference extends MemberReference {
    private final FieldData referencedField;

    public FieldReference(FileLocation location, FieldData referencedField) {
        super(location);
        this.referencedField = Objects.requireNonNull(referencedField);
        Preconditions.checkArgument(
                location.size() == this.getName().length(),
                "Field is size %s, but has name %s @ %s",
                location.size(),
                this.getName(),
                location
        );
    }

    @Override
    public String getName() {
        return referencedField.getName();
    }

    @Override
    public FieldReference remap(Mappings mappings) {
        FieldData newField = mappings.getNewField(referencedField);
        return new FieldReference(
                new FileLocation(this.getStart(), this.getStart() + newField.getName().length()),
                newField
        );
    }

    @Override
    @Nullable
    public String remapName(FastMappings mappings) {
        FastMappings.ClassMappings classMappings = mappings.getClassMappings(this.getReferencedField().getDeclaringType().getInternalName());
        if (classMappings != null) {
            return classMappings.getFieldName(this.getName());
        }
        return null;
    }

    public void serialize(ByteBuf output) {
        this.getLocation().serialize(output);
        SerializationUtils.writePrefixedString(output, referencedField.getInternalName());
    }
    public static FieldReference deserialize(ByteBuf in) {
        int startIndex = in.readerIndex();
        FileLocation location = FileLocation.deserialize(in);
        String referencedFieldName = SerializationUtils.readPrefixedString(in);
        final FieldData referencedField;
        try {
            referencedField = FieldData.fromInternalName(referencedFieldName);
        } catch (IllegalArgumentException e) {
            throw new IllegalArgumentException(
                    "Invalid data: "
                            + ByteBufUtil.hexDump(in, startIndex, in.readerIndex() - startIndex)
                            + " with internal name "
                            + new JsonPrimitive(referencedFieldName).toString()
            );
        }
        return new FieldReference(
                location,
                referencedField
        );
    }
    public static FieldReference createRandom(Random random) {
        FieldData referencedField = RandomUtils.randomField(random);
        int start = random.nextInt(1000);
        int end = start + referencedField.getName().length();
        return new FieldReference(
                new FileLocation(start, end),
                referencedField
        );
    }
    @Override
    public String toString() {
        return referencedField.getInternalName() + "@" + getLocation();
    }
}
