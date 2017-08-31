package net.techcable.supersrg.utils;

import org.junit.Assert;
import org.junit.Test;

public class TextUtilsTest {
    @Test
    public void testParseNestedParens() {
        String text = "('everybody', 'wants', (('too',) 'rule'), 'the world')Eats";
        Assert.assertEquals(
                text.lastIndexOf("Eats") - 1,
                TextUtils.findClosingDelimiter(text, 0, '(', ')')
        );
    }
}
