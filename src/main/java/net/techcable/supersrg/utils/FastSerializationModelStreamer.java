package net.techcable.supersrg.utils;

import lombok.*;
import spoon.reflect.ModelStreamer;
import spoon.reflect.factory.Factory;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;

import io.netty.buffer.ByteBuf;

import org.nustaq.serialization.FSTConfiguration;
import org.nustaq.serialization.FSTObjectInput;
import org.nustaq.serialization.FSTObjectOutput;

public class FastSerializationModelStreamer implements ModelStreamer {

    public void save(Factory f, OutputStream out) throws IOException {
        FSTObjectOutput serializer = new FSTObjectOutput(out);
        serializer.writeObject(f);
        serializer.flush();
        serializer.close();
    }

    public Factory load(ByteBuf buffer) {
        byte[] bytes = new byte[buffer.readableBytes()];
        buffer.readBytes(bytes);
        FSTConfiguration config =FSTConfiguration.createDefaultConfiguration();
        final Factory f = (Factory) config.asObject(bytes);
        this.setupFactory(f);
        return f;
    }

    @SneakyThrows(ClassNotFoundException.class)
    public Factory load(InputStream in) throws IOException {
        FSTObjectInput ois = new FSTObjectInput(in);
        final Factory f = (Factory) ois.readObject();
        setupFactory(f);
        ois.close();
        return f;
    }

    private void setupFactory(Factory f) {
        //create query using factory directly
        //because any try to call CtElement#map or CtElement#filterChildren will fail on uninitialized factory
        f.createQuery(f.getModel().getRootPackage()).filterChildren(e -> {
            e.setFactory(f);
            return false;
        }).list();
    }
}
