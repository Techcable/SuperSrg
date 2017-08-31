package net.techcable.supersrg.source;

import spoon.processing.AbstractProcessor;
import spoon.reflect.declaration.CtElement;
import spoon.reflect.declaration.CtField;
import spoon.reflect.declaration.CtMethod;
import spoon.reflect.declaration.CtType;
import spoon.reflect.reference.CtExecutableReference;
import spoon.reflect.reference.CtFieldReference;

import java.util.Objects;
import java.util.Set;
import javax.annotation.Nullable;

import com.google.common.collect.ImmutableSet;

import net.techcable.srglib.FieldData;
import net.techcable.srglib.MethodData;

public class MemberReferenceExtractor extends AbstractProcessor<CtElement> {
    private final RangeMapBuilder builder;
    @Nullable
    private final ProgressPrintingProcessor progressProcessor;

    public MemberReferenceExtractor(RangeMapBuilder builder, @Nullable ProgressPrintingProcessor progressProcessor) {
        this.builder = Objects.requireNonNull(builder);
        this.progressProcessor = progressProcessor;
    }

    private static final ImmutableSet<Class<? extends CtElement>> PROCESSED_ELEMENT_TYPES = ImmutableSet.of(
            CtExecutableReference.class,
            CtFieldReference.class,
            CtMethod.class,
            CtField.class,
            CtType.class
    );

    @Override
    public Set<Class<? extends CtElement>> getProcessedElementTypes() {
        return PROCESSED_ELEMENT_TYPES;
    }

    @Override
    public void process(CtElement element) {
        if (element instanceof CtExecutableReference) {
            processExecutableReference((CtExecutableReference) element);
        } else if (element instanceof CtFieldReference) {
            processFieldReference((CtFieldReference) element);
        } else if (element instanceof CtMethod) {
            processMethodDeclaration((CtMethod) element);
        } else if (element instanceof CtField) {
            processFieldDeclaration((CtField) element);
        } else if (element instanceof CtType) {
            if (progressProcessor != null) {
                progressProcessor.process((CtType) element);
            }
        } else {
            throw new UnsupportedOperationException("Unknown type: " + element);
        }
    }

    private void processFieldDeclaration(CtField element) {
        FileLocation location = SpoonUtils.getNameLocation(element);
        FieldData fieldData = SpoonUtils.getFieldData(element.getReference());
        String actualText = SpoonUtils.getTextAt(
                SpoonUtils.getOrInferFile(element),
                location
        );
        if (!actualText.equals(fieldData.getName())) {
            throw new IllegalArgumentException("Expected " + fieldData.getName() + ": " + actualText);
        }
        if (fieldData.getInternalName().isEmpty()) {
            throw new IllegalArgumentException("Empty name: " + fieldData.getInternalName());
        }
        builder.addFieldReference(
                SpoonUtils.getFileName(element),
                new FieldReference(location, fieldData)
        );
    }

    private void processMethodDeclaration(CtMethod element) {
        FileLocation location = SpoonUtils.getNameLocation(element);
        MethodData originalData = SpoonUtils.getMethodData(element.getReference());
        if (originalData != null) {
            String actualText = SpoonUtils.getTextAt(
                    SpoonUtils.getOrInferFile(element),
                    location
            );
            if (!actualText.equals(originalData.getName())) {
                throw new IllegalArgumentException("Expected " + originalData.getName() + ": " + actualText);
            }
            builder.addMethodReference(
                    SpoonUtils.getFileName(element),
                    new MethodReference(location, originalData)
            );
        }
    }

    private void processFieldReference(CtFieldReference element) {
        FileLocation location = SpoonUtils.getNameLocation(element);
        if (location != null) {
            FieldData fieldData = SpoonUtils.getFieldData(element);
            String actualText = SpoonUtils.getTextAt(
                    SpoonUtils.getOrInferFile(element),
                    location
            );
            if (!actualText.equals(fieldData.getName())) {
                throw new IllegalArgumentException("Expected " + fieldData.getName() + ": " + actualText);
            }
            if (fieldData.getInternalName().isEmpty()) {
                throw new IllegalArgumentException("Empty name: " + fieldData.getInternalName());
            }
            builder.addFieldReference(
                    SpoonUtils.getFileName(element),
                    new FieldReference(location, fieldData)
            );
        }
    }

    private void processExecutableReference(CtExecutableReference element) {
        if (element.isConstructor()) return;
        FileLocation location = SpoonUtils.getNameLocation(element);
        if (location != null) {
            MethodData originalData = SpoonUtils.getMethodData(element);
            if (originalData != null) {
                String actualText = SpoonUtils.getTextAt(
                        SpoonUtils.getOrInferFile(element),
                        location
                );
                if (!actualText.equals(originalData.getName())) {
                    throw new IllegalArgumentException("Expected " + originalData.getName() + ": " + actualText);
                }
                builder.addMethodReference(
                        SpoonUtils.getFileName(element),
                        new MethodReference(location, originalData)
                );
            }
        }
    }
}
