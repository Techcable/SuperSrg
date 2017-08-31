package net.techcable.supersrg.source;

import spoon.processing.AbstractProcessor;
import spoon.reflect.CtModel;
import spoon.reflect.declaration.CtType;

public class ProgressPrintingProcessor extends AbstractProcessor<CtType> {
    private final int numClasses;
    public ProgressPrintingProcessor(CtModel model) {
        this.numClasses = model.getAllTypes().size();
    }

    private int lastPercentage = 0;
    private int numProcessed = 0;
    @Override
    public void process(CtType element) {
        if (!element.isTopLevel()) return;
        double percentage = (((double) numProcessed++) / ((double) numClasses)) * 100;
        int integerPercentage = (int) percentage;
        if (integerPercentage != lastPercentage) {
            lastPercentage = integerPercentage;
            System.out.print("\r" + integerPercentage + "% complete.");
            System.out.flush();
        }
    }
}
