package net.techcable.supersrg.cmd;

@FunctionalInterface
public interface CheckedRunnable {
    void run() throws Throwable;
}
