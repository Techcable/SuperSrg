package net.techcable.supersrg.utils;

import java.util.StringJoiner;

import org.junit.Test;

import static net.techcable.supersrg.utils.IgnoreSet.*;
import static org.junit.Assert.*;

public class IgnoreSetTest {
    @Test
    public void testPreserveUncommented() {
        assertEquals("Preserved", stripComments("Preserved"));
        assertEquals("Multiline\nPreserved", stripComments("Multiline\nPreserved"));
    }
    @Test
    public void testStripSingleLine() {
        assertEquals("", stripComments("// Bob"));
        assertEquals("\nSecond", stripComments("// First\nSecond"));
        assertEquals("Trailing \nnextline", stripComments("Trailing // comment\nnextline"));
        assertEquals("Comment on\n", stripComments("Comment on\n// Second line"));
        assertEquals("We can\nintersperse\nwith code", stripComments("We can// Try to\nintersperse// comments\nwith code// Yay!"));
    }
    @Test
    public void testStripBlock() {
        assertEquals("Prefix", stripComments("Prefix/*Comments \n are fun */"));
        assertEquals("Suffix", stripComments("/* Even for food */Suffix"));
        assertEquals("PrefixSuffix", stripComments("Prefix/* Block comments \n aren't as good */Suffix/* As single \n line */"));
    }
    @Test
    public void testIndexOfUncommented() {
        assertEquals(9, IgnoreSet.findComments("/* Bob */the").ignoringIndexOf("the", 0));
        {
            String bigText = new StringJoiner("\n")
                    .add("// Paper start")
                    .add("/*")
                    .add("public static void main(String[] args) {")
                    .add("    runCommentedOutCode()")
                    .add("} */")
                    .add("// Paper end")
                    .add(" /* Random block comment */")
                    .add("public static void actualMain(String[] args) {")
                    .toString();
            IgnoreSet comments = IgnoreSet.findComments(bigText);
            assertEquals(bigText.lastIndexOf('('), comments.ignoringIndexOf("(", 0));
            assertEquals(-1, comments.ignoringIndexOf("(", comments.ignoringIndexOf("(", 0) + 1));
        }
    }
    @Test
    public void testStripAnnotations() {
        assertEquals(" Foo", IgnoreSet.findAnnotations("@Bob Foo").stripIgnored());
        assertEquals("\nQueen", IgnoreSet.findAnnotations("@Taco(\"Eats\")\nQueen").stripIgnored());
        assertEquals(
                "\n code()",
                IgnoreSet.findAnnotations("@Metadata({value = \"that rocks\", truth = true}, bob = ({\"The buidler\"}))\n@Eating code()").stripIgnored()
        );
    }
}
