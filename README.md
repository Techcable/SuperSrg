SuperSrg
=========
The ultimate srg remapper, supporting both jar and source remapping.

**NOTE:** I'm unlikely to continue to maintain this since i've somewhat lost intrest in minecraft,
but I think some people may find it useful as I used it to get MCP working.

## Features
- Directly remap source files, using spoon AST.
  - The necessary information is extracted from the AST into a RangeMap file,
    which can be used to apply mappings very fast
- Java portion uses [SrgLib library](https://github.com/Techcable/SrgLib) to parse mappings
- Native binary to generate minecraft mappings and apply rangemaps

## TODO
- Access transforms
