package net.techcable.supersrg.source;

import javax.annotation.Nonnull;
import javax.annotation.Nullable;

import net.techcable.srglib.mappings.Mappings;
import net.techcable.supersrg.utils.FastMappings;

import org.eclipse.jdt.core.dom.MemberRef;

public abstract class MemberReference implements Comparable<MemberReference> {
    private final int start, end;
    protected MemberReference(FileLocation location) {
        this.start = location.getStart();
        this.end = location.getEnd();
    }
    public final int getStart() {
        return start;
    }

    public final int getEnd() {
        return end;
    }

    public final int getSize() {
        return end - start;
    }

    public final FileLocation getLocation() {
        return new FileLocation(start, end);
    }

    public abstract String getName();

    public abstract MemberReference remap(Mappings mappings);

    @Nullable
    public abstract String remapName(FastMappings mappings);

    @Override
    public int compareTo(@Nonnull MemberReference o) {
        return getLocation().compareTo(o.getLocation());
    }
}
