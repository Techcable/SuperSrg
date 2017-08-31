package net.techcable.supersrg.source;

import lombok.*;
import spoon.SpoonException;
import spoon.reflect.code.CtExecutableReferenceExpression;
import spoon.reflect.code.CtFieldAccess;
import spoon.reflect.code.CtInvocation;
import spoon.reflect.code.CtTargetedExpression;
import spoon.reflect.cu.CompilationUnit;
import spoon.reflect.cu.SourcePosition;
import spoon.reflect.declaration.CtElement;
import spoon.reflect.declaration.CtEnumValue;
import spoon.reflect.declaration.CtExecutable;
import spoon.reflect.declaration.CtField;
import spoon.reflect.declaration.CtMethod;
import spoon.reflect.declaration.CtNamedElement;
import spoon.reflect.declaration.CtParameter;
import spoon.reflect.declaration.CtType;
import spoon.reflect.reference.CtExecutableReference;
import spoon.reflect.reference.CtFieldReference;
import spoon.reflect.reference.CtParameterReference;
import spoon.reflect.reference.CtReference;
import spoon.reflect.reference.CtTypeParameterReference;
import spoon.reflect.reference.CtTypeReference;

import java.io.File;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;
import java.util.concurrent.ExecutionException;
import javax.annotation.Nullable;

import com.google.common.base.Preconditions;
import com.google.common.base.Verify;
import com.google.common.cache.Cache;
import com.google.common.cache.CacheBuilder;
import com.google.common.collect.ImmutableList;
import com.google.common.io.Files;

import net.techcable.srglib.FieldData;
import net.techcable.srglib.JavaType;
import net.techcable.srglib.MethodData;
import net.techcable.supersrg.utils.IgnoreSet;
import net.techcable.supersrg.utils.TextUtils;

public class SpoonUtils {

    private SpoonUtils() {}

    public static JavaType getJavaType(CtTypeReference typeRef) {
        if (typeRef instanceof CtTypeParameterReference) {
            CtTypeReference boundingType = ((CtTypeParameterReference) typeRef).getBoundingType();
            if (boundingType != null) {
                return getJavaType(boundingType);
            } else {
                return getJavaType(typeRef.getTypeErasure());
            }
        } else {
            return JavaType.fromName(typeRef.getQualifiedName());
        }
    }

    public static FieldData getFieldData(CtFieldReference fieldRef) {
        return FieldData.create(
                getJavaType(fieldRef.getDeclaringType()),
                fieldRef.getSimpleName()
        );
    }
    @Nullable
    public static MethodData getMethodData(CtExecutableReference<?> methodReference) {
        if (methodReference.isConstructor()) return null;
        List<CtTypeReference<?>> parameterTypeRefs = methodReference.getParameters();
        int numParameters = parameterTypeRefs.size();
        JavaType[] parameterTypes = new JavaType[numParameters];
        for (int i = 0; i < numParameters; i++) {
            CtTypeReference<?> parameterTypeRef = parameterTypeRefs.get(i);
            try {
                parameterTypes[i] = getJavaType(parameterTypeRef);
            } catch (SpoonException e) {
                CtExecutable declaration = methodReference.getDeclaration();
                if (declaration == null) {
                    declaration = methodReference.getExecutableDeclaration();
                }
                if (declaration != null) {
                    // Try again with the parameter from the declaration itself
                    CtParameter parameterDeclaration = (CtParameter) declaration.getParameters().get(i);
                    parameterTypes[i] = getJavaType(parameterDeclaration.getType());
                } else {
                    throw e;
                }
            }
        }
        JavaType returnType;
        try {
            returnType = getJavaType(methodReference.getType());
        } catch (SpoonException e) {
            CtExecutable declaration = methodReference.getExecutableDeclaration();
            if (declaration == null) {
                declaration = methodReference.getExecutableDeclaration();
            }
            if (declaration != null) {
                returnType = getJavaType(declaration.getType());
            } else {
                throw e;
            }
        }
        return MethodData.create(
                getJavaType(methodReference.getDeclaringType()),
                methodReference.getSimpleName(),
                ImmutableList.copyOf(parameterTypes),
                returnType
        );
    }

    public static FileLocation getNameLocation(CtNamedElement element) {
        SourcePosition position = element.getPosition();
        if (position.getSourceStart() < 0) {
            throw new IllegalArgumentException("Element doesn't have position: " + element);
        }
        String text = getTextAt(element);
        IgnoreSet ignored = IgnoreSet.findComments(text).union(IgnoreSet.findAnnotations(text));
        String name = element.getSimpleName();
        if (element instanceof CtMethod) {
            // Find the end of the name
            int nameEnd = ignored.ignoringIndexOf("(", 0);
            // Strip whitespace
            while (TextUtils.isAsciiWhitespace(text.charAt(nameEnd - 1))) {
                nameEnd -= 1;
            }
            Verify.verify(nameEnd >= 0, "Unable to find parameter start: " + text);
            String beforeParameters = text.substring(0, nameEnd);
            if (!beforeParameters.endsWith(name)) {
                throw new IllegalArgumentException("Text before parameters doesn't end with " + name + ": " + text);
            }
            int offset = beforeParameters.length() - name.length();
            Verify.verify(offset >= 0);
            return new FileLocation(
                    position.getSourceStart() + offset,
                    position.getSourceStart() + offset + name.length()
            );
        } else if (element instanceof CtField && !(element instanceof CtEnumValue)) {
            int nameEnd = ignored.ignoringIndexOf("=", 0);
            if (nameEnd < 0) {
                nameEnd = ignored.ignoringIndexOf(";", 0);
            }
            Verify.verify(nameEnd >= 0, "Unable to detect name end: %s", text);
            // Strip whitespace
            while (TextUtils.isAsciiWhitespace(text.charAt(nameEnd - 1))) {
                nameEnd -= 1;
            }
            String beforeEnd = text.substring(0, nameEnd);
            if (!beforeEnd.endsWith(name)) {
                // Handle multi-field declarations like 'int min, max'
                int separatorIndex = beforeEnd.lastIndexOf(',');
                while (separatorIndex >= 0) {
                    // Strip whitespace
                    while (TextUtils.isAsciiWhitespace(text.charAt(separatorIndex - 1))) {
                        nameEnd -= 1;
                    }
                    beforeEnd = beforeEnd.substring(0, separatorIndex);
                    if (beforeEnd.endsWith(name)) break;
                    separatorIndex = beforeEnd.lastIndexOf(',', separatorIndex);
                }
                if (!beforeEnd.endsWith(name)) {
                    throw new IllegalArgumentException("Name text doesn't end with " + name + ": " + text);
                }
            }
            int offset = beforeEnd.length() - name.length();
            return new FileLocation(
                    position.getSourceStart() + offset,
                    position.getSourceStart() + offset + name.length()
            );
        } else if (element instanceof CtEnumValue) {
            int initializerEnd = ignored.ignoringIndexOf("(", 0);
            int anonymousClassOpen = ignored.ignoringIndexOf("{", 0);
            int nameEnd;
            if (anonymousClassOpen >= 0 && anonymousClassOpen < initializerEnd) {
                // We may not have an initializer at all, so only consider params that come before the anonymous class
                nameEnd = anonymousClassOpen;
            } else if (initializerEnd >= 0) {
                nameEnd = initializerEnd;
            } else {
                nameEnd = text.length();
            }
            // Strip whitespace
            while (TextUtils.isAsciiWhitespace(text.charAt(nameEnd - 1))) {
                nameEnd -= 1;
            }
            String beforeEnd = text.substring(0, nameEnd);
            if (!beforeEnd.endsWith(name)) {
                throw new IllegalArgumentException("Text before initializers doesn't end with " + name + ": " + text);
            }
            int offset = beforeEnd.length() - name.length();
            return new FileLocation(
                    position.getSourceStart() + offset,
                    position.getSourceStart() + offset + name.length()
            );
        } else {
            throw new UnsupportedOperationException("Unsupported element: " + element);
        }
    }
    @Nullable
    public static FileLocation getNameLocation(CtReference element) {
        SourcePosition position = element.getPosition();
        // NOTE: Don't use getLine/getColumn, since apparently they're slow
        if (position.getSourceStart() < 0) {
            CtElement parent = element;
            while (parent != null) {
                if (parent.isImplicit()) {
                    return null;
                }
                parent = parent.getParent();
            }
            return fixPosition(element);
        } else if (element instanceof CtFieldReference) {
            String text = getTextAt(element);
            if (!text.endsWith(element.getSimpleName())) {
                throw new IllegalArgumentException("Expected " + element.getSimpleName() + ", but got " + text);
            }
            return new FileLocation(
                    position.getSourceEnd() + 1 - element.getSimpleName().length(),
                    position.getSourceEnd() + 1
            );
        } else {
            String text = getTextAt(element);
            if (!text.startsWith(element.getSimpleName())) {
                throw new IllegalArgumentException("Text doesn't start with " + element.getSimpleName() + ": " + text);
            }
            return new FileLocation(
                    position.getSourceStart(),
                    position.getSourceStart() + element.getSimpleName().length()
            );
        }
    }
    private static File cachedFile;
    private static byte[] cachedFileContents;
    @SneakyThrows(IOException.class)
    private  static byte[] loadBytes(File file) {
        if (file.equals(cachedFile)) {
            return cachedFileContents;
        }
        byte[] result = Files.asByteSource(file).read();
        cachedFileContents = result;
        cachedFile = file;
        return result;
    }
    @Nullable
    public static String tryGetTextAt(CtElement element) {
        return element.getPosition().getSourceStart() >= 0 ? getTextAt(element) : null;
    }
    private static final Cache<String, CompilationUnit> cachedCompilationUnits = CacheBuilder.newBuilder()
            .maximumSize(10_000)
            .softValues()
            .build();
    @Setter
    private static ImmutableList<File> sourceRoots = ImmutableList.of();
    @SneakyThrows(ExecutionException.class)
    public static CompilationUnit inferCompilationUnit(CtElement element) {
        CtType rootType = element instanceof CtType ? ((CtType) element).getTopLevelType() : element.getParent(CtType.class).getTopLevelType();
        if (rootType.getPosition().getFile() != null) {
            return Objects.requireNonNull(rootType.getPosition().getCompilationUnit());
        }
        String qualifiedName = rootType.getQualifiedName();
        return cachedCompilationUnits.get(qualifiedName, () -> {
            String relativeName = qualifiedName.replace('.', '/') + ".java";
            for (File sourceRoot : sourceRoots) {
                File sourceFile = new File(sourceRoot, relativeName);
                if (sourceFile.exists()) {
                    return element.getFactory().CompilationUnit().create(sourceFile.getPath());
                }
            }
            return null;
        });
    }
    public static File getOrInferFile(CtElement element) {
        File f = element.getPosition().getFile();
        if (f != null) return f;
        CompilationUnit compilationUnit = element.getPosition().getCompilationUnit();
        if (compilationUnit == null) {
            compilationUnit = inferCompilationUnit(element);
            if (compilationUnit == null) {
                throw new IllegalArgumentException("Unable to infer compilation unit: " + element);
            }
        }
        return compilationUnit.getFile();
    }
    public static String getTextAt(CtElement element) {
        Preconditions.checkArgument(element.getPosition().getSourceStart() >= 0, "Invalid position!");
        SourcePosition position = element.getPosition();
        return getTextAt(getOrInferFile(element), new FileLocation(position.getSourceStart(), position.getSourceEnd() + 1));
    }
    public static String getTextAt(File file, FileLocation location) {
        byte[] byteData = new byte[location.size()];
        System.arraycopy(
                loadBytes(file),
                location.getStart(),
                byteData,
                0,
                byteData.length
        );
        return new String(byteData, StandardCharsets.UTF_8);
    }
    public static FileLocation fixPosition(CtReference reference) {
        CtElement validParent = reference.getParent();
        while (validParent.getParent() != null && validParent.getPosition().getSourceStart() < 0) {
            validParent = validParent.getParent();
        }
        final CtTargetedExpression logicalParent;
        // NOTE: CtExecutableReferenceExpression is the class of a lambda reference
        if (validParent instanceof CtInvocation || validParent instanceof CtExecutableReferenceExpression || validParent instanceof CtFieldAccess) {
            logicalParent = (CtTargetedExpression) validParent;
        } else {
            logicalParent = (CtTargetedExpression) validParent.getParent(
                    (CtElement e) -> e instanceof CtInvocation || e instanceof CtExecutableReference || e instanceof CtFieldAccess
            );
        }
        if (reference instanceof CtExecutableReference && reference.getParent() instanceof CtParameterReference) {
            // A CtParameterReference holds an implicit reference to it's parent, but it's not marked as implicit
            Verify.verify(
                    ((CtParameterReference) reference.getParent()).getDeclaringExecutable().equals(reference),
                    "Expected unmarked implicit reference to %s: %s",
                    reference,
                    reference.getParent()
            );
            return null;
        }
        SourcePosition parentPosition = validParent.getPosition();
        if ((reference instanceof CtExecutableReference || reference instanceof CtFieldReference) && logicalParent != null) {
            // Manually fix this
            String parentText = getTextAt(logicalParent);
            String name = reference.getSimpleName();
            int index = parentText.indexOf(name);
            if (parentText.indexOf(name, index + name.length()) >= 0) {
                /*
                 * Attempt to further disambiguate the call, in the event that the target text also contains the name.
                 * This can happen for example, in chained calls to StringBuilder like `builder.append("bob").append("foo")`
                 * The target is the `builder.append("bob")` call, which we've already handled, so we need to only handle the second one.
                 */
                String targetText = tryGetTextAt(logicalParent.getTarget());
                int targetStartIndex = targetText != null ? parentText.indexOf(targetText) : -1;
                int targetEndIndex = targetStartIndex >= 0 ? targetStartIndex + targetText.length() : 0;
                /*
                 * Strip any type parameters in nested angle brackets <>, if there are any.
                 * However, we have to be careful to only look before the start of method parameters '(',
                 * or else we may run into shift expressions and crash
                 */
                int firstAngleBracket = parentText.indexOf('<', targetEndIndex);
                int parametersStart = parentText.indexOf('(', index + name.length());
                final int lastAngleBracket;
                if (firstAngleBracket >= 0 && firstAngleBracket < parametersStart) {
                    int nesting = 1;
                    int currentIndex = firstAngleBracket;
                    do {
                        currentIndex += 1;
                        if (currentIndex >= parentText.length()) {
                            throw new IllegalArgumentException(
                                    "Unable to resolve nested angle brackets after "
                                            + firstAngleBracket
                                            + ": " + parentText
                            );
                        }
                        char c = parentText.charAt(currentIndex);
                        if (c == '<') {
                            nesting += 1;
                        } else if (c == '>') {
                            nesting -= 1;
                        }
                    } while (nesting > 0);
                    lastAngleBracket = currentIndex;
                    Verify.verify(
                            parentText.charAt(lastAngleBracket) == '>',
                            "Expected last angle bracket, but got %s",
                            parentText.charAt(lastAngleBracket)
                    );
                } else {
                    lastAngleBracket = targetEndIndex;
                }
                index = parentText.indexOf(name, lastAngleBracket);
                /*
                 * Also handle the case that a variable is named the same as the method,
                 * like `terminal(terminal) by looking for the '(' that starts the method's parameters.
                 */
                parametersStart = parentText.indexOf('(', index + name.length());
                int offendingIndex = parentText.indexOf(name, index + name.length());
                if (offendingIndex >= 0 && (parametersStart < 0 || offendingIndex < parametersStart)) {
                    throw new IllegalArgumentException(
                            "Unable to fix position since multiple occurrences of name "
                                    + name
                                    + " in "
                                    + parentText
                                    + "@"
                                    + parentText
                    );
                }
            }
            if (index < 0) {
                throw new IllegalArgumentException("Unable to find " + name + ": " + parentText);
            }
            // We only have a single occurrence
            return new FileLocation(
                    parentPosition.getSourceStart() + index,
                    parentPosition.getSourceStart() + index + name.length()
            );
        }
        CtType type = reference.getParent(CtType.class);
        String typeName = type != null ? type.getQualifiedName() : "unknown";
        throw new IllegalArgumentException(
                reference.getClass().getSimpleName()
                        + " doesn't have position in "
                        + typeName
                        + " with first valid parent "
                        + validParent.getClass().getSimpleName()
                        + " "
                        + getTextAt(validParent)
                        + "@"
                        + validParent.getPosition()
                        + ": "
                        + reference
        );
    }
    public static String getFileName(CtElement element) {
        CtType declaringType = element.getParent(CtType.class);
        String typeName = declaringType.getQualifiedName();
        String packageName;
        if (declaringType.getPackage() != null) {
            packageName = declaringType.getPackage().getQualifiedName();
        } else {
            packageName = typeName.substring(0, typeName.lastIndexOf('.'));
        }
        // Determine relative className of the root file based on file path and package name
        return packageName.replace('.', '/') + "/" + getOrInferFile(declaringType).getName();
    }
}
