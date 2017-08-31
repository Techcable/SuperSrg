from typing import Dict, Tuple, Callable, List
from abc import ABCMeta, abstractmethod
from enum import Enum, unique
from sys import intern
from collections import defaultdict

class JavaType(metaclass=ABCMeta):
    @property
    @abstractmethod
    def descriptor(self) -> str:
        pass

    @staticmethod
    def parse(text) -> "JavaType":
        value, actual_length = self.partially_parse_descriptor(text)
        if actual_length < len(text):
            raise ValueError(f"Unexpected trailing {repr(text[length:])}: {text}")
        return value

    @staticmethod
    def partially_parse_descriptor(text) -> Tuple["JavaType", int]:
        try:
            first_char = text[0]
        except IndexError:
            raise ValueError("Empty descriptor!")
        if first_char == 'L':
            try:
                end = text.index(";")
                return JavaClass(text[1:end]), end + 1
            except ValueError:
                raise ValueError("fMissing ending semicolon for class descriptor: {text}")
        elif first_char == '[':
            element_descriptor = text.lstrip("[")
            dimensions = len(text) - len(element_descriptor)
            element_type, element_size = JavaType.partially_parse_descriptor(element_descriptor)
            return JavaArray(dimensions, element_type), dimensions + element_size
        else:
            try:
                return JavaPrimitive(PrimitiveType(first_char)), 1
            except ValueError:
                raise ValueError(f"Invalid descriptor: {text}") from None

class JavaArray(JavaType):
    __slots__ = "dimensions", "element_type"
    def __init__(self, dimensions, element_type):
        assert dimensions >= 1
        assert not isinstance(element_type, JavaArray)
        self.dimensions = dimensions
        self.element_type = element_type

    def __eq__(self, other):
        if isinstance(other, JavaArray):
            return self.element_type == other.element_type and self.dimensions == other.dimensions
        elif isinstance(other, JavaType):
            # We can safely compare against other non-array JavaTypes, but we'll always be false
            assert self.descriptor != other.descriptor
            return False
        else:
            return NotImplemented

    def __hash__(self):
        return hash(self.element_type) + self.dimensions

    @property
    def descriptor(self):
        return ("[" * self.dimensions) + self.element_type.descriptor

@unique
class PrimitiveType(Enum):
    BYTE = 'B'
    SHORT = 'S'
    INT = 'I'
    LONG = 'J'
    FLOAT = 'F'
    DOUBLE = 'D'
    CHAR = 'C'
    BOOLEAN = 'Z'
    VOID = 'V'

    @property
    def descriptor(self):
        return self.value

class JavaPrimitive(JavaType):
    __slots__ = "primitive"
    def __init__(self, primitive):
        assert isinstance(primitive, PrimitiveType)
        self.primitive = primitive

    def __hash__(self):
        return hash(self.primitive)

    def __eq__(self, other):
        if isinstance(other, JavaPrimitive):
            return self.primitive == other.primitive
        elif isinstance(other, JavaType):
            # We can safely compare against other non-primitive JavaTypes, but we'll always be false
            assert self.descriptor != other.descriptor
            return False
        else:
            return NotImplemented

    @property
    def descriptor(self):
        return self.primitive.descriptor

class JavaClass(JavaType):
    __slots__ = "internal_name"
    def __init__(self, internal_name):
        self.internal_name = intern(internal_name)
    
    def __hash__(self):
        return hash(self.internal_name)

    def __eq__(self, other):
        if isinstance(other, JavaClass):
            return self.internal_name == other.internal_name
        elif isinstance(other, JavaType):
            # We can safely compare against other non-class JavaTypes, but we'll always be false
            return False
        else:
            return NotImplemented 

    @property
    def descriptor(self):
        return f"L{self.internal_name};"

class MethodSignature:
    __slots__ = "descriptor", "return_type", "prameter_types"
    descriptor: str
    return_type: JavaType
    parameter_types: List[JavaType]
    def __init__(self, return_type, parameter_types, descriptor=None):
        parameter_types = tuple(parameter_types)
        if descriptor is None:
            descriptor_parts = ["("]
            for parameter in parameter_types:
                assert parameter != JavaPrimitive(PrimitiveType.VOID), "Void parameter"
                descriptor_parts.append(parameter.descriptor)
            descriptor_parts.append(")")
            descriptor_parts.append(return_type.descriptor)
            descriptor = "".join(descriptor_parts)
        self.descriptor = intern(descriptor) # NOTE: Intern descriptors too
        self.return_type = return_type
        self.parameter_types = parameter_types

    def __hash__(self):
        return hash(self.descriptor)

    def __eq__(self, other):
        return self.descriptor == other.descriptor

    def __repr__(self):
        return f"MethodSignature.parse({repr(self.descriptor)})"        

    @staticmethod
    def parse(descriptor):
        if descriptor[0] != '(':
            raise ValueError(f"Missing opening paren: {descriptor}")
        index = 1
        try:
            parameter_end = descriptor.index(')')
        except ValueError:
            raise ValueError(f"Missing closing paren: {descriptor}")
        parameter_types = []
        while index < parameter_end:
            parameter_type, size = JavaType.partially_parse_descriptor(descriptor[1:parameter_end])
            if parameter_type == PrimitiveType(JavaPrimitive.VOID):
                raise ValueError(f"Void parameter #{len(parameter_types)}: {descriptor}")
            parameter_types.append(parameter_type)
            index += size
        assert index == parameter_end
        return_type = JavaType.parse(descriptor[parameter_end + 1:])
        return MethodSignature(return_type, parameter_types, descriptor)

class MethodData:
    __slots__ = "declaring_class", "name", "signature"
    declaring_class: JavaClass
    name: str
    signature: MethodSignature
    def __init__(self, declaring_class, name, signature):
        assert type(declaring_class) is JavaClass, f"Unexpected class: {repr(declaring_class)}"
        assert type(signature) is MethodSignature, f"Unexpected signature: {repr(signature)}"
        self.declaring_class = declaring_class
        self.name = intern(name)
        self.signature = signature

    def __eq__(self, other):
        if isinstance(other, MethodData):
            return self.declaring_class.internal_name == other.declaring_class.internal_name \
                and self.name == other.name \
                and self.signature.descriptor == other.signature.descriptor
        else:
            return NotImplemented

    def __hash__(self):
        return hash((self.declaring_class.internal_name, self.name, self.signature.descriptor))

    @property
    def internal_name(self):
        return f"{self.declaring_class.internal_name}/{self.name}"

class FieldData:
    __slots__ = "declaring_class", "name"
    def __init__(self, declaring_class, name):
        assert type(declaring_class) is JavaClass, f"Unexpected class: {repr(declaring_class)}"
        self.declaring_class = declaring_class
        self.name = intern(name)

    def __eq__(self, other):
        if isinstance(other, FieldData):
            return other.declaring_class.internal_name == self.declaring_class.internal_name and self.name == other.name
        else:
            return NotImplemented

    def __hash__(self):
        return hash((self.declaring_class.internal_name, self.name))

    @property
    def internal_name(self):
        return f"{self.declaring_class.internal_name}/{self.name}"

class MappingsBuilder:
    __slots__ = "classes", "method_names", "field_names"
    classes: Dict[JavaClass, JavaClass]
    method_names: Dict[MethodData, str]
    field_names: Dict[FieldData, str]
    def __init__(self):
        self.classes = {}
        self.method_names = {}
        self.field_names = {}

    def build(self):
        classes = {}

class Mappings:
    __slots__ = "classes", "methods", "fields", "_signature_cache"
    classes: Dict[JavaClass, JavaClass]
    methods: Dict[MethodData, MethodData]
    fields: Dict[FieldData, FieldData]
    def __init__(self, classes, methods, fields):
        self.classes = classes
        self.methods = methods
        self.fields = fields
        self._signature_cache = defaultdict(self._remap_signature)

    def _remap_signature(self):
        return MethodSignature(self[original.return_type], map(self.__getitem__, original.parameter_types))

    def __getitem__(self, key):
        if isinstance(key, JavaType):
            if isinstance(key, JavaClass):
                # Lookup the remapped class, returning the original if missing
                return self.classes.get(key, key)
            elif isinstance(key, JavaPrimitive):
                return key  # Ignore primitives
            elif isinstance(key, JavaArray):
                # Remap the undelring element type, only if it's a JavaClass
                element_type = key.element_type
                if isinstance(element_type, JavaClass):
                    return JavaArray(key.dimensions, self.classes.get(element_type, element_type))
                else:
                    assert not isinstance(element_type, JavaArray), "Nested array"
            else:
                raise TypeError(f"Unexpected JavaType: {repr(key)}")
        elif isinstance(key, MethodData):
            result = self.methods.get(key)
            if result is not None:
                return result
            else:
                original_class = key.declaring_class
                remapped_signature = self._signature_cache[original.signature]
                remapped_class = self.classes.get(original_class, original_class)
                return MethodData(remapped_class, original.name, remapped_signature)
        elif isinstance(key, FieldData):
            result = self.fields.get(key)
            if result is not None:
                return result
            else:
                original_class = key.declaring_class
                remapped_class = self.classes.get(original_class, original_class)
                return FieldData(original_class, original.name)
        else:
            raise TypeError(f"Unexpected key type: {repr(key)}")
