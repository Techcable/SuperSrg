package net.techcable.supersrg.source;

import java.util.Objects;
import java.util.Random;

import io.netty.buffer.ByteBuf;

import org.junit.Assert;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.junit.runners.Parameterized;

@RunWith(Parameterized.class)
public class RangeMapSerializeTest {
    private final RangeMap rangeMap;

    public RangeMapSerializeTest(RangeMap rangeMap) {
        this.rangeMap = Objects.requireNonNull(rangeMap);
    }

    @Test
    public void testRoundtrip() {
        ByteBuf data = rangeMap.serialize();
        RangeMap deserialized = RangeMap.deserialize(data);
        Assert.assertEquals(rangeMap, deserialized);
    }

    @Parameterized.Parameters
    public static RangeMap[] testData() {
        RangeMap[] result = new RangeMap[3];
        Random random = new Random();
        for (int i = 0; i < result.length; i++) {
            result[i] = RangeMap.createRandom(random);
        }
        return result;
    }
}
