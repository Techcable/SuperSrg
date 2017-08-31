
import msgpack
from typing import Dict, Optional, Sequence, Iterator, Tuple
import io
import os
import itertools
import json
import bisect
from array import array

class RangeMap:
    def __init__(self, files: Dict[str, "FileRanges"]):
        self.files = files

    @staticmethod
    def load(source):
        if isinstance(source, io.IOBase):
            data = msgpack.load(source)
        elif isinstance(source, (str, os.PathLike)):
            with open(source, 'rb') as f:
                data = msgpack.load(f)
        else:
            raise TypeError(f"Unexpeted type: {type(source)}")
        return RangeMap.deserialize(data)

    @staticmethod
    def deserialize(data):
        files = {}
        for raw_name, file_hash in data[b'fileHashes'].items():
            file_name = raw_name.decode('utf-8')
            assert file_name not in files
            files[file_name] = FileRanges(file_hash, (), ())
        for raw_name, raw_references in data[b'methodReferences'].items():
            file_name = raw_name.decode('utf-8')
            references = tuple(map(MethodReference.deserialize, raw_references))
            try:
                files[file_name].method_references = references
            except KeyError:
                files[file_name] = FileRanges(None, references, ())
        for raw_name, raw_references in data[b'fieldReferences'].items():
            file_name = raw_name.decode('utf-8')
            references = tuple(map(FieldReference.deserialize, raw_references))
            try:
                files[file_name].field_references = references
            except KeyError:
                files[file_name] = FileRanges(None, (), references)
        return RangeMap(files)

    @property
    def all_method_references(self) -> Iterator[Tuple[str, "MethodReference"]]:
        for file_name, ranges in self.files.items():
            for method_reference in ranges.method_references:
                yield (file_name, method_reference)

    @property
    def all_field_references(self) -> Iterator[Tuple[str, "FieldReference"]]:
        for file_name, ranges in self.files.items():
            for field_reference in ranges.field_references:
                yield (file_name, field_reference)

    def __repr__(self):
        return f"RangeMap(files={repr(self.files)})"

class FileRanges:
    __slots__ = "file_hash", "method_references", "field_references"
    file_hash: Optional[bytes]
    method_references: Sequence["MethodReference"]
    field_references: Sequence["FieldReference"]
    def __init__(self, file_hash, method_references, field_references):
        self.file_hash = file_hash
        self.method_references = method_references
        self.field_references = field_references

    def __repr__(self):
        result = ["FileRanges(file_hash="]
        result.append(repr(self.file_hash))
        result.append(", method_references=")
        result.append(repr([repr(ref) for ref in self.method_references]))
        result.append(", field_references=")
        result.append(repr([repr(ref) for ref in self.field_references]))
        result.append(")")
        return ''.join(result)
        
class MethodReference:
    __slots__ = "location", "name", "signature"
    def __init__(self, location, name, signature):
        self.location = location
        self.name = name
        self.signature = signature

    @staticmethod
    def deserialize(data):
        start = int(data[:4].hex(), 16)
        end = int(data[4:8].hex(), 16)
        name_length = int(data[8:10].hex(), 16)
        index = 10
        name = data[index:index + name_length].decode('utf-8')
        index += name_length
        signature_length = int(data[index:index + 2].hex(), 16)
        index += 2
        signature = data[index:index + signature_length].decode('utf-8')
        index += signature_length
        assert index == len(data), f"Unexpected end to data: {data}"
        return MethodReference(FileLocation(start, end), name, signature)

    def __str__(self):
        return f"{self.name}{self.signature}@{self.location}"

    def __repr__(self):
        return f"MethodReference({self.location}, {self.name}, {self.signature})"

class FieldReference:
    __slots__ = "location", "name"
    def __init__(self, location, name):
        self.location = location
        self.name = name

    @staticmethod
    def deserialize(data):
        start = int(data[:4].hex(), 16)
        end = int(data[4:8].hex(), 16)
        name_length = int(data[8:10].hex(), 16)
        index = 10
        name = data[index:index + name_length].decode('utf-8')
        index += name_length
        assert index == len(data), f"Unexpected end to data: {data}"
        return FieldReference(FileLocation(start, end), name)

    def __str__(self):
        return f"{self.name}@{self.location}"

    def __repr__(self):
        return f"FieldReference({self.location}, {self.name})"

class FileLocation:
    __slots__ = "start", "end", "_line"
    def __init__(self, start: int, end: int):
        self.start = start
        self.end = end

    lineOffsets = {}
    def determineLine(self, path):
        try:
            return self._line
        except AttributeError:
            pass
        try:
            offsets = FileLocation.lineOffsets[path]
        except KeyError:
            with open(path, 'rb') as f:
                data = f.read()
                fileIndex = 0
                offsets = array('L', [0])
                while True:
                    try:
                        fileIndex = data.index(b'\n', fileIndex) + 1
                        offsets.append(fileIndex)
                    except ValueError:
                        break
                FileLocation.lineOffsets[path] = offsets
        line = bisect.bisect_left(offsets, self.start)
        self._line = line
        return line

    def __str__(self):
        return f"{self.start}:{self.end}"

    def __repr__(self):
        return f"FileLocation({self.start}, {self.end})"
