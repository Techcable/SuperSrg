from io import TextIOBase
from . import Mappings

class SrgMappingsEncoder:
    __slots__ = "output"
    def __init__(self, output: TextIOBase):
        self.output = output

    def encode(self, mappings: Mappings):
        output = self.output
        for original, renamed in mappings.classes.items():
            output.write("CL: ")
            output.write(original.internal_name)
            output.write(" ")
            output.write(renamed.internal_name)
            output.write("\n")
        for original, renamed in mappings.fields.items():
            output.write("FD: ")
            output.write(original.declaring_class.internal_name)
            output.write('/')
            output.write(original.name)
            output.write(" ")
            output.write(renamed.declaring_class.internal_name)
            output.write('/')
            output.write(renamed.name)
            output.write("\n")
        for original, renamed in mappings.methods.items():
            output.write("MD: ")
            output.write(original.declaring_class.internal_name)
            output.write('/')
            output.write(original.name)
            output.write(' ')
            output.write(original.signature.descriptor)
            output.write(' ')
            output.write(renamed.declaring_class.internal_name)
            output.write('/')
            output.write(renamed.name)
            output.write(' ')
            output.write(renamed.signature.descriptor)
            output.write('\n')
    
