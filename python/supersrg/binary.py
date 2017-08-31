from sys import intern

from . import MappingsBuilder, MethodData, FieldData, JavaClass, MethodSignature

try:
    from lz4.block import decompress as lz4_decompress
except ImportError:
    lz4_decompress = None


class BinaryMappingsError(Exception):
    pass

class BinaryMappingsDecoder:
    """SuperSrg binary mappings decoder"""
    __slots__ = "data"
    def __init__(self, data):
        # NOTE: We are forced to read everything into memory by the lz4 implementation
        assert isinstance(data, bytes, bytearray), f"Unexpected data: {repr(data)}"
        self.data = data
        # NOTE: Taking a memoryview allows O(1) slicing, which would otherwise imply a copy
        # However, we still need to keep the underlying bytes object for utilities like 'index'
        self.data_view = memoryview(data)
        self.index = 0

    def read_uint(self, amount):
        try:
            old_index = self.index
            end_index = old_index + amount
            result = int.from_bytes(self.data_view[old_index:end_index], 'big')
            self.index = end_index
            return result
        except IndexError:
            raise BinaryMappingsError("Insufficent data!") from None

    def read_string(self):
        length = self.read_uint(2)
        try:
            index = self.index
            return str(self.data_view[index:index + length], 'utf-8')
        except IndexError:
            raise BinaryMappingsError(f"Insufficent data to read {length} byte string") from None
        except UnicodeError as e:
            raise BinaryMappingsError(f"Invalid {length} byte string!") from e

    def read_u32(self):
        return self.read_uint(4)

    def read_u64(self):
        return self.read_uint(8)

    def read_nullterm(self):
        try:
            data = self.data
            end = data.index('\0', self.index)
            self.index = end + 1  # Jump one past the null terminator
            return str(self.data_view[:end], 'utf-8')
        except ValueError, UnicodeError as e:
            raise BinaryMappingsError("Unable to read null terminated string!") from e

    def decode(self) -> MappingsBuilder:
        try:
            header = self.read_nullterm()
        except BinaryMappingsError as e:
            raise BinaryMappingsError("Invalid header!") from e.__cause__
        if header != "SuperSrg binary mappings":
            raise BinaryMappingsError(f"Unexpected header: {header}")
        version = self.read_u32()
        if version != 1:
            raise BinaryMappingsError(f"Unexpected version: {version}")
        compression = self.read_string()
        if compression == "":
            # Continue to treat uncompressed data as-is
            pass
        elif compression == "lz4-block":
            if lz4_decompress is None:
                raise BinaryMappingsError(f"Missing lz4 compression module!")
            decompressed = lz4_decompress(self.data_view[index:])
            self.data = decompressed
            self.data_view = memoryview(decompressed)
            self.index = 0
        elif compression in ("lzma2", "gzip"):
            raise BinaryMappingsError(f"Unsupported compression: {compression}")
        else:
            raise BinaryMappingsError(f"Forbidden compression: {compression}")
        builder = MappingsBuilder()
        num_classes = self.read_u64()
        for _ in range(num_classes):
            original_class = JavaClass(self.read_string())
            revised_class_name = self.read_string()
            revised_class = JavaClass(revised_class_name) if revised_class_name else original_class
            num_methods = self.read_u32()
            for _ in range(num_methods):
                original_name = self.read_string()
                revised_name = self.read_string()
                original_signature = MethodSignature.parse(self.read_string())
                self.read_string()  # Ignore the revised signature
                original_data = MethodData(original_class, original_name, original_signature)
                builder.method_names[original_data] = intern(revised_name)
            num_fields = self.read_u32()
            for _ in range(num_fields):
                original_name = self.read_string()
                revised_name = self.read_string()
                original_data = FieldData(original_class, original_name)
                assert original_name != revised_name, f"Redundant field: {original_data}"
                builder.field_names[original_data] = intern(revised_name)
        return builder

