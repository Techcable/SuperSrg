package net.techcable.supersrg.source;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.Objects;

import com.google.common.base.Preconditions;
import com.google.common.base.Verify;
import com.google.common.collect.ImmutableList;

import net.techcable.supersrg.utils.FastMappings;

public class StreamRangeApplier {
    private final InputStream in;
    private final OutputStream out;
    private final ImmutableList<MemberReference> references;

    public StreamRangeApplier(
            InputStream in,
            OutputStream out,
            List<MemberReference> references
    ) {
        this.in = Objects.requireNonNull(in);
        this.out = Objects.requireNonNull(out);
        this.references = ImmutableList.sortedCopyOf(references);
    }

    public void apply(FastMappings mappings) throws IOException {
        byte[] buffer = new byte[4096];
        int numReferences = references.size();
        int fileIndex = 0;
        for (int referenceIndex = 0; referenceIndex < numReferences; referenceIndex++) {
            MemberReference reference = references.get(referenceIndex);
            int startIndex = reference.getStart();
            if (fileIndex > startIndex) {
                if (referenceIndex > 0) {
                    MemberReference lastReference = references.get(referenceIndex - 1);
                    Preconditions.checkState(
                            !lastReference.getLocation().hasOverlap(reference.getLocation()),
                            "Overlapping references: %s and %s",
                            reference,
                            lastReference
                    );
                }
                throw new IllegalStateException("File index " + fileIndex + " overran next reference " + reference);
            }
            while (fileIndex < startIndex) {
                int toCopy = Math.min(buffer.length, startIndex - fileIndex);
                int numRead = in.read(buffer, 0, toCopy);
                if (numRead < 0) {
                    throw new IllegalStateException("Unexpected EOF @ " + fileIndex);
                }
                out.write(buffer, 0, numRead);
                fileIndex += numRead;
            }
            Verify.verify(fileIndex == startIndex);
            int referenceSize = reference.getSize();
            Verify.verify(referenceSize <= buffer.length);
            int bufferSize = 0;
            while (bufferSize < referenceSize) {
                int numRead = in.read(buffer, bufferSize, referenceSize - bufferSize);
                if (numRead < 0) {
                    throw new IllegalStateException("Unexpected EOF @ " + (fileIndex + bufferSize));
                }
                bufferSize += numRead;
            }
            Verify.verify(bufferSize == referenceSize);
            String actualName = new String(buffer, 0, bufferSize, StandardCharsets.UTF_8);
            if (!actualName.equals(reference.getName())) {
                throw new IllegalStateException("Expected " + reference.getName() + ", but got " + actualName);
            }
            String remappedName = reference.remapName(mappings);
            if (remappedName == null) {
                remappedName = reference.getName();
            }
            out.write(remappedName.getBytes(StandardCharsets.UTF_8));
            fileIndex += bufferSize;
        }
    }
}
