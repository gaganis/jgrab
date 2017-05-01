package com.athaydes.jgrab.runner;

import com.athaydes.jgrab.Dependency;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.Scanner;
import java.util.stream.Stream;

/**
 * A class that knows how to extract information from the source code of a Java file through the standard input.
 */
class StdinJavaCode implements JavaCode {

    private static final Logger logger = LoggerFactory.getLogger( StdinJavaCode.class );

    private final String className;
    private final String[] lines;

    StdinJavaCode() throws IOException {
        this.lines = readLinesFromStdin();
        this.className = JavaCode.extractClassNameFrom( lines );
        logger.debug( "Class name: {}", className );
    }

    private static String[] readLinesFromStdin() {
        Scanner scanner = new Scanner( System.in );
        List<String> result = new ArrayList<>();

        while ( scanner.hasNextLine() ) {
            result.add( scanner.nextLine() );
        }

        return result.toArray( new String[ 0 ] );
    }

    @Override
    public boolean isSnippet() {
        return false; // Java files must be classes
    }

    @Override
    public List<Dependency> extractDependencies() {
        return Dependency.parseDependencies( Stream.of( lines ) );
    }

    @Override
    public String getClassName() {
        return className;
    }

    @Override
    public String getCode() {
        return String.join( "\n", lines );
    }

}
