import org.gradle.api.tasks.compile.JavaCompile
import org.gradle.jvm.tasks.Jar
import com.github.jengelman.gradle.plugins.shadow.tasks.ShadowJar

plugins {
    id("java")
    id("maven")
    id("com.github.johnrengelman.shadow").version("2.0.0")
}
group = "net.techcable"
version = "0.1.0"
repositories {
    mavenCentral()
    maven {
        name = "techcable-repo"
        setUrl("https://repo.techcable.net/content/groups/public")
    }
    maven {
        name = "spoon"
        setUrl("http://spoon.gforge.inria.fr/repositories/releases")
    }
}
val nettyVersion = "4.1.12.Final"
dependencies {
    compile("com.google.guava:guava:22.0") // Guava
    compile("com.google.code.gson:gson:2.8.0") // Gson - Json parsing
    compile("org.ow2.asm:asm-debug-all:5.2") // ASM - Classfile manipulation
    compile("org.ow2.asm:asm-commons:5.2") // ASM - Classfile manipulation
    compile("net.techcable:srglib:0.1.2") // SrgLib - Srg parsing and manipulation
    compile("fr.inria.gforge.spoon:spoon-core:5.7.0") // Spoon - Java AST manipulation
    compile("net.sourceforge.argparse4j:argparse4j:0.7.0") // Argparse4J - Argument parsing
    compile("org.msgpack:msgpack-core:0.8.13") // Msgpack - Binary data serialization
    compile("io.netty:netty-buffer:$nettyVersion") // Netty buffer - binary buffers
    compile("net.jpountz.lz4:lz4:1.3-SNAPSHOT") // Lz4 SNAPSHOT - fast compression
    compile("org.apache.commons:commons-compress:1.14") // Commons compress has proper support for Lz4's frame format
    compile("de.ruedigermoeller:fst:2.51") // Fast serialization
    compileOnly("org.projectlombok:lombok:1.16.16")
    compileOnly("com.google.code.findbugs:jsr305:1.3.9")
    testCompile("junit:junit:4.12")
}
val shadowJar: ShadowJar by tasks
shadowJar.apply {
    baseName = "SuperSrg"
    version = null
}
tasks["build"].dependsOn(shadowJar)
tasks.withType<JavaCompile> {
    sourceCompatibility = "1.8"
    targetCompatibility = "1.8"
    options.encoding = "UTF-8"
}
