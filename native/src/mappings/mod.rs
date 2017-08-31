use std::borrow::{Cow, Borrow};
use std::fmt::{self, Formatter};

use string_cache::DefaultAtom;
use ordermap::OrderMap;
use parking_lot::RwLock;

use types::{PooledFieldData, FieldDataLookup, MethodDataLookup, PooledMethodData, PooledJavaClass, MethodSignature, JavaClassLookup};
use utils::{SeaHashOrderMap, PooledString};

pub mod binary;
pub mod parser;
pub mod encoder;
pub mod utils;

pub trait MappingsTransformer {
    #[inline]
    fn transform_class<T: JavaClassLookup>(&self, _: &T) -> Option<Cow<PooledJavaClass>> {
        None
    }
    #[inline]
    fn transform_method<T: MethodDataLookup>(&self, _: &T) -> Option<Cow<DefaultAtom>> {
        None
    }
    #[inline]
    fn transform_field<T: FieldDataLookup>(&self, _: &T) -> Option<Cow<DefaultAtom>> {
        None
    }
}
#[derive(Default, Clone)]
pub struct MappingsBuilder {
    pub field_names: SeaHashOrderMap<PooledFieldData, DefaultAtom>,
    pub method_names: SeaHashOrderMap<PooledMethodData, DefaultAtom>,
    pub classes: SeaHashOrderMap<PooledJavaClass, PooledJavaClass>,
}
impl MappingsBuilder {
    #[inline]
    pub fn new() -> MappingsBuilder {
        Default::default()
    }
    #[inline]
    pub fn with_capacities(classes: usize, fields: usize, methods: usize) -> MappingsBuilder {
        MappingsBuilder {
            method_names: OrderMap::with_capacity_and_hasher(methods, Default::default()),
            field_names: OrderMap::with_capacity_and_hasher(fields, Default::default()),
            classes: OrderMap::with_capacity_and_hasher(classes, Default::default()),
        }
    }
    #[inline]
    pub fn insert_class(&mut self, original_class: PooledJavaClass, new_class: PooledJavaClass) -> &mut Self {
        self.classes.insert(original_class, new_class);
        self
    }
    #[inline]
    pub fn insert_field(&mut self, original_field: PooledFieldData, new_name: DefaultAtom) -> &mut Self {
        self.field_names.insert(original_field, new_name);
        self
    }
    #[inline]
    pub fn insert_method(&mut self, original_method: PooledMethodData, new_name: DefaultAtom) -> &mut Self {
        self.method_names.insert(original_method, new_name);
        self
    }
    /// Chain the specified mappings to the output of this builder
    pub fn chain<'a, M: MappingsIterator<'a>>(&mut self, mappings: M) {
        // TODO: Somehow apply this without copying
        let mut reversed_builder = self.clone();
        reversed_builder.reverse();
        let reversed = reversed_builder.snapshot();
        for (chained_original, renamed) in mappings.classes() {
            let chained_original = chained_original.borrow();
            let original = reversed
                .try_get_class(chained_original)
                .unwrap_or(chained_original)
                .clone();
            self.classes.insert(original, renamed.clone());
        }
        for (chained_original, renamed) in mappings.field_names() {
            let chained_original = chained_original.borrow();
            let original = reversed.get_field(chained_original);
            self.field_names.insert(original, renamed.clone());
        }
        for (chained_original, renamed) in mappings.method_names() {
            let chained_original = chained_original.borrow();
            let original = reversed.get_method(chained_original);
            self.method_names.insert(original, renamed.clone());
        }
    }
    /// Apply the specified transformation to the mappings in-place
    pub fn transform<T: MappingsTransformer>(&mut self, transformer: &T) {
        // TODO: Use already built Mappings type here, instead of rebuiding each item in-place
        // TODO: Somehow add a marker to indicate whether we actually desire field/method transformations
        for (original_field, revised_name) in self.field_names.iter_mut() {
            let original_class = &original_field.class;
            let new_class = self.classes
                .get(original_class)
                .unwrap_or(original_class)
                .clone();
            let revised_data = PooledFieldData {
                class: new_class,
                name: revised_name.clone(),
            };
            if let Some(changed_name) = transformer.transform_field(&revised_data) {
                *revised_name = changed_name.into_owned();
            }
        }
        let signatures = self.compute_signatures();
        for (original_method, revised_name) in self.method_names.iter_mut() {
            let original_class = &original_method.class;
            let new_class = self.classes
                .get(original_class)
                .unwrap_or(original_class)
                .clone();
            let new_signature = signatures.get(&original_method.signature).expect(
                "Missing signature",
            );
            let revised_data = PooledMethodData {
                class: new_class,
                name: revised_name.clone(),
                signature: new_signature.clone(),
            };
            if let Some(changed_name) = transformer.transform_method(&revised_data) {
                *revised_name = changed_name.into_owned();
            }
        }
        // NOTE: Classes must be remapped _after_ fields and methods so we don't clobber the old data when we need it
        for revised_class in self.classes.values_mut() {
            if let Some(changed_class) = transformer.transform_class(revised_class) {
                *revised_class = changed_class.into_owned();
            }
        }
    }
    fn compute_signatures(&self) -> SeaHashOrderMap<PooledString, DefaultAtom> {
        let mut signatures: SeaHashOrderMap<PooledString, DefaultAtom> = Default::default();
        {
            signatures.clear();
            let mut descriptor_buf = String::new();
            for original in self.method_names.keys() {
                let old_signature = &original.signature;
                signatures
                    .entry(PooledString(old_signature.clone()))
                    .or_insert_with(|| {
                        descriptor_buf.clear();
                        MethodSignature::new(old_signature)
                            .parse()
                            .expect("Invalid descriptor")
                            .remap_class(|original_class| {
                                self.classes
                                    .get(original_class)
                                    .map(PooledJavaClass::borrowed)
                                    .unwrap_or(*original_class)
                            })
                            .write_descriptor(&mut descriptor_buf);
                        assert!(!descriptor_buf.is_empty()); // Paranoia
                        DefaultAtom::from(descriptor_buf.as_ref())
                    });
            }
        }
        signatures
    }
    pub fn reverse(&mut self) {
        let num_methods = self.method_names.len();
        let mut reversed_method_names = OrderMap::with_capacity_and_hasher(num_methods, Default::default());
        for (original, renamed) in self.methods() {
            reversed_method_names.insert(renamed.into_owned(), original.name.clone());
        }
        self.method_names = reversed_method_names;
        let num_fields = self.field_names.len();
        let mut reversed_field_names = OrderMap::with_capacity_and_hasher(num_fields, Default::default());
        for (original, renamed) in self.fields() {
            reversed_field_names.insert(renamed.into_owned(), original.name.clone());
        }
        self.field_names = reversed_field_names;
        let num_classes = self.classes.len();
        let mut reversed_classes = OrderMap::with_capacity_and_hasher(num_classes, Default::default());
        // NOTE: Must reverse the classes last
        for (original, renamed) in self.classes() {
            reversed_classes.insert(renamed.clone(), original.clone());
        }
        self.classes = reversed_classes;
    }
}
impl Mappings for MappingsBuilder {
    fn snapshot(&self) -> MappingsSnapshot {
        let mut fields: SeaHashOrderMap<PooledFieldData, PooledFieldData> = SeaHashOrderMap::with_capacity_and_hasher(self.field_names.len(), Default::default());
        let mut methods: SeaHashOrderMap<PooledMethodData, PooledMethodData> = SeaHashOrderMap::with_capacity_and_hasher(self.method_names.len(), Default::default());
        let mut classes: SeaHashOrderMap<PooledJavaClass, PooledJavaClass> = SeaHashOrderMap::with_capacity_and_hasher(self.classes.len(), Default::default());
        // Copying classes is simple, just borrow them
        for (old_class, new_class) in &self.classes {
            classes.insert(old_class.clone(), new_class.clone());
        }
        for (original, new_name) in &self.field_names {
            let original_class = original.class.clone();
            let new_class = classes
                .get(&original_class)
                .unwrap_or(&original_class)
                .clone();
            fields.insert(
                original.clone(),
                PooledFieldData {
                    class: new_class,
                    name: new_name.clone(),
                },
            );
        }
        let signatures = self.compute_signatures();
        for (original, new_name) in &self.method_names {
            let original_class = &original.class;
            let new_class = classes
                .get(original_class)
                .unwrap_or(original_class)
                .clone();
            let old_signature = &original.signature;
            let new_signature = signatures.get(old_signature).expect("Missing signature");
            methods.insert(
                original.clone(),
                PooledMethodData {
                    class: new_class,
                    name: new_name.clone(),
                    signature: new_signature.clone(),
                },
            );
        }
        MappingsSnapshot {
            fields,
            methods,
            classes,
            signature_cache: RwLock::new(signatures),
        }
    }
    #[inline]
    fn try_get_method_name<T: MethodDataLookup>(&self, original: &T) -> Option<&DefaultAtom> {
        self.method_names.get(original)
    }
    #[inline]
    fn try_get_field_name<T: FieldDataLookup>(&self, original: &T) -> Option<&DefaultAtom> {
        self.field_names.get(original)
    }
    #[inline]
    fn try_get_field<T: FieldDataLookup>(&self, original: &T) -> Option<Cow<PooledFieldData>> {
        if let Some(renamed_name) = self.field_names.get(original) {
            Some(Cow::Owned(PooledFieldData {
                name: renamed_name.clone(),
                class: self.get_class(original.class()),
            }))
        } else {
            None
        }
    }
    #[inline]
    fn try_get_method<T: MethodDataLookup>(&self, original: &T) -> Option<Cow<PooledMethodData>> {
        if let Some(renamed_name) = self.method_names.get(original) {
            Some(Cow::Owned(PooledMethodData {
                name: renamed_name.clone(),
                class: self.get_class(original.class()),
                signature: self.remap_signature(&original.pooled_signature()),
            }))
        } else {
            None
        }
    }
    #[inline]
    fn try_get_class<T: JavaClassLookup>(&self, original: &T) -> Option<&PooledJavaClass> {
        self.classes.get(original)
    }
}
impl<'a> MappingsIterator<'a> for &'a MappingsBuilder {
    type Classes = ::ordermap::Iter<'a, PooledJavaClass, PooledJavaClass>;
    type Fields = MappingsBuilderFieldIter<'a>;
    type Methods = MappingsBuilderMethodIter<'a>;
    type FieldNames = ::ordermap::Iter<'a, PooledFieldData, DefaultAtom>;
    type MethodNames = ::ordermap::Iter<'a, PooledMethodData, DefaultAtom>;
    #[inline]
    fn classes(self) -> Self::Classes {
        self.classes.iter()
    }
    #[inline]
    fn fields(self) -> Self::Fields {
        MappingsBuilderFieldIter(self, self.field_names.iter())
    }
    #[inline]
    fn methods(self) -> Self::Methods {
        // Recompute the signature cache
        let signatures = self.compute_signatures();
        MappingsBuilderMethodIter {
            builder: self,
            handle: self.method_names.iter(),
            signature_cache: signatures,
        }
    }
    #[inline]
    fn field_names(self) -> Self::FieldNames {
        self.field_names.iter()
    }
    #[inline]
    fn method_names(self) -> Self::MethodNames {
        self.method_names.iter()
    }
    #[inline]
    fn num_classes(self) -> usize {
        self.classes.len()
    }
    #[inline]
    fn num_fields(self) -> usize {
        self.field_names.len()
    }
    #[inline]
    fn num_methods(self) -> usize {
        self.method_names.len()
    }
    #[inline]
    fn rebuild(self) -> MappingsBuilder {
        self.clone()
    }
}
pub struct MappingsBuilderFieldIter<'a>(&'a MappingsBuilder, ::ordermap::Iter<'a, PooledFieldData, DefaultAtom>);
impl<'a> Iterator for MappingsBuilderFieldIter<'a> {
    type Item = (&'a PooledFieldData, Cow<'a, PooledFieldData>);
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some((original, renamed_name)) = self.1.next() {
            let renamed = PooledFieldData {
                class: self.0.get_class(&original.class),
                name: renamed_name.clone(),
            };
            Some((original, Cow::Owned(renamed)))
        } else {
            None
        }
    }
}
pub struct MappingsBuilderMethodIter<'a> {
    builder: &'a MappingsBuilder,
    handle: ::ordermap::Iter<'a, PooledMethodData, DefaultAtom>,
    signature_cache: SeaHashOrderMap<PooledString, DefaultAtom>,
}
impl<'a> Iterator for MappingsBuilderMethodIter<'a> {
    type Item = (&'a PooledMethodData, Cow<'a, PooledMethodData>);
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some((original, renamed_name)) = self.handle.next() {
            let renamed_signature = self.signature_cache.get(&original.signature).expect(
                "Missing signature",
            );
            let renamed = PooledMethodData {
                name: renamed_name.clone(),
                signature: renamed_signature.clone(),
                class: self.builder.get_class(&original.class),
            };
            Some((original, Cow::Owned(renamed)))
        } else {
            None
        }
    }
}
pub trait Mappings {
    fn snapshot(&self) -> MappingsSnapshot;
    /// Try and get the remapped field name if it exists
    fn try_get_field_name<T: FieldDataLookup>(&self, original: &T) -> Option<&DefaultAtom>;
    /// Try and get the remapped field if it exists
    fn try_get_field<T: FieldDataLookup>(&self, original: &T) -> Option<Cow<PooledFieldData>>;
    /// Get the remapped field if it exists, remapping the original field's class if not
    #[inline]
    fn get_field<T: FieldDataLookup>(&self, original: &T) -> PooledFieldData {
        if let Some(renamed) = self.try_get_field(original) {
            renamed.into_owned()
        } else {
            PooledFieldData {
                name: original.pooled_name().into_owned(),
                class: self.get_class(original.class()),
            }
        }
    }
    /// Try and get the remapped ,ethod name if it exists
    fn try_get_method_name<T: MethodDataLookup>(&self, original: &T) -> Option<&DefaultAtom>;
    /// Try and get the remapped method if it exists
    fn try_get_method<T: MethodDataLookup>(&self, original: &T) -> Option<Cow<PooledMethodData>>;
    /// Get the remapped method if it exists, remapping the original methods's classes if not
    #[inline]
    fn get_method<T: MethodDataLookup>(&self, original: &T) -> PooledMethodData {
        if let Some(renamed) = self.try_get_method(original) {
            renamed.into_owned()
        } else {
            PooledMethodData {
                name: original.pooled_name().into_owned(),
                class: self.get_class(original.class()),
                signature: self.remap_signature(&original.pooled_signature().into_owned()),
            }
        }
    }
    fn try_get_class<T: JavaClassLookup>(&self, original: &T) -> Option<&PooledJavaClass>;
    /// Get the remapped class, returning the original if it doesn't exist
    #[inline]
    fn get_class<T: JavaClassLookup>(&self, original: &T) -> PooledJavaClass {
        self.try_get_class(original)
            .map(PooledJavaClass::clone)
            .unwrap_or_else(|| original.intern())
    }
    fn remap_signature(&self, original: &DefaultAtom) -> DefaultAtom {
        // NOTE: Default implementation never caches, so it's valid even for a mutable MappingsBuilder
        let parsed = MethodSignature::new(original).parse().expect(
            "Invalid signature",
        );
        let remapped_descriptor = parsed
            .remap_class(|original| {
                self.try_get_class(original)
                    .map(|x| x.borrowed())
                    .unwrap_or(*original)
            })
            .descriptor();
        if remapped_descriptor == original.as_ref() {
            // no need to intern again
            original.clone()
        } else {
            DefaultAtom::from(remapped_descriptor)
        }
    }
}
pub trait MappingsIterator<'a>: Sized + Copy {
    type Classes: Iterator<Item = (&'a PooledJavaClass, &'a PooledJavaClass)>;
    type Fields: Iterator<Item = (&'a PooledFieldData, Cow<'a, PooledFieldData>)>;
    type Methods: Iterator<Item = (&'a PooledMethodData, Cow<'a, PooledMethodData>)>;
    type FieldNames: Iterator<Item = (&'a PooledFieldData, &'a DefaultAtom)>;
    type MethodNames: Iterator<Item = (&'a PooledMethodData, &'a DefaultAtom)>;
    fn classes(self) -> Self::Classes;
    fn fields(self) -> Self::Fields;
    fn methods(self) -> Self::Methods;
    fn field_names(self) -> Self::FieldNames;
    fn method_names(self) -> Self::MethodNames;
    fn rebuild(self) -> MappingsBuilder {
        let mut builder = MappingsBuilder::with_capacities(self.num_classes(), self.num_fields(), self.num_methods());
        for (original, renamed) in self.classes() {
            builder.insert_class(original.clone(), renamed.clone());
        }
        for (original, renamed) in self.field_names() {
            builder.insert_field(original.clone(), renamed.clone());
        }
        for (original, renamed) in self.method_names() {
            builder.insert_method(original.clone(), renamed.clone());
        }
        builder
    }
    fn num_classes(self) -> usize;
    fn num_fields(self) -> usize;
    fn num_methods(self) -> usize;
}
pub struct MappingsSnapshotMethodNameIter<'a>(::ordermap::Iter<'a, PooledMethodData, PooledMethodData>);
impl<'a> Iterator for MappingsSnapshotMethodNameIter<'a> {
    type Item = (&'a PooledMethodData, &'a DefaultAtom);
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some((original, renamed)) = self.0.next() {
            Some((original, &renamed.name))
        } else {
            None
        }
    }
}
pub struct MappingsSnapshotFieldNameIter<'a>(::ordermap::Iter<'a, PooledFieldData, PooledFieldData>);
impl<'a> Iterator for MappingsSnapshotFieldNameIter<'a> {
    type Item = (&'a PooledFieldData, &'a DefaultAtom);
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some((original, renamed)) = self.0.next() {
            Some((original, &renamed.name))
        } else {
            None
        }
    }
}
pub struct MappingsSnapshot {
    fields: SeaHashOrderMap<PooledFieldData, PooledFieldData>,
    methods: SeaHashOrderMap<PooledMethodData, PooledMethodData>,
    classes: SeaHashOrderMap<PooledJavaClass, PooledJavaClass>,
    signature_cache: RwLock<SeaHashOrderMap<PooledString, DefaultAtom>>,
}
impl Mappings for MappingsSnapshot {
    #[inline]
    fn snapshot(&self) -> MappingsSnapshot {
        self.clone()
    }
    #[inline]
    fn try_get_method_name<T: MethodDataLookup>(&self, original: &T) -> Option<&DefaultAtom> {
        self.methods.get(original).map(|x| &x.name)
    }
    #[inline]
    fn try_get_field_name<T: FieldDataLookup>(&self, original: &T) -> Option<&DefaultAtom> {
        self.fields.get(original).map(|x| &x.name)
    }
    #[inline]
    fn try_get_field<T: FieldDataLookup>(&self, original: &T) -> Option<Cow<PooledFieldData>> {
        self.fields.get(original).map(Cow::Borrowed)
    }
    /// Try and get the remapped method if it exists
    #[inline]
    fn try_get_method<T: MethodDataLookup>(&self, original: &T) -> Option<Cow<PooledMethodData>> {
        self.methods.get(original).map(Cow::Borrowed)
    }
    #[inline]
    fn try_get_class<T: JavaClassLookup>(&self, original: &T) -> Option<&PooledJavaClass> {
        self.classes.get(original)
    }
    fn remap_signature(&self, original: &DefaultAtom) -> DefaultAtom {
        {
            let lock = self.signature_cache.read();
            if let Some(cached) = lock.get(original) {
                return cached.clone();
            }
        }
        self.compute_remapped_signature(original)
    }
}
impl Clone for MappingsSnapshot {
    fn clone(&self) -> MappingsSnapshot {
        let fields = self.fields.clone();
        let methods = self.methods.clone();
        let classes = self.classes.clone();
        let lock = self.signature_cache.read();
        let signature_cache = RwLock::new(lock.clone());
        MappingsSnapshot {
            classes,
            fields,
            methods,
            signature_cache,
        }
    }
}
impl<'a> MappingsIterator<'a> for &'a MappingsSnapshot {
    type Classes = ::ordermap::Iter<'a, PooledJavaClass, PooledJavaClass>;
    type Fields = MappingsSnapshotFieldsIter<'a>;
    type Methods = MappingsSnapshotMethodsIter<'a>;
    type FieldNames = MappingsSnapshotFieldNameIter<'a>;
    type MethodNames = MappingsSnapshotMethodNameIter<'a>;
    #[inline]
    fn classes(self) -> Self::Classes {
        self.classes.iter()
    }
    #[inline]
    fn fields(self) -> Self::Fields {
        MappingsSnapshotFieldsIter(self.fields.iter())
    }
    #[inline]
    fn methods(self) -> Self::Methods {
        MappingsSnapshotMethodsIter(self.methods.iter())
    }
    #[inline]
    fn field_names(self) -> Self::FieldNames {
        MappingsSnapshotFieldNameIter(self.fields.iter())
    }
    #[inline]
    fn method_names(self) -> Self::MethodNames {
        MappingsSnapshotMethodNameIter(self.methods.iter())
    }
    #[inline]
    fn num_classes(self) -> usize {
        self.classes.len()
    }
    #[inline]
    fn num_fields(self) -> usize {
        self.fields.len()
    }
    #[inline]
    fn num_methods(self) -> usize {
        self.fields.len()
    }
}
pub struct MappingsSnapshotFieldsIter<'a>(::ordermap::Iter<'a, PooledFieldData, PooledFieldData>);
impl<'a> Iterator for MappingsSnapshotFieldsIter<'a> {
    type Item = (&'a PooledFieldData, Cow<'a, PooledFieldData>);
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some((original, renamed)) = self.0.next() {
            Some((original, Cow::Borrowed(renamed)))
        } else {
            None
        }
    }
}
pub struct MappingsSnapshotMethodsIter<'a>(::ordermap::Iter<'a, PooledMethodData, PooledMethodData>);
impl<'a> Iterator for MappingsSnapshotMethodsIter<'a> {
    type Item = (&'a PooledMethodData, Cow<'a, PooledMethodData>);
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some((original, renamed)) = self.0.next() {
            Some((original, Cow::Borrowed(renamed)))
        } else {
            None
        }
    }
}

impl MappingsSnapshot {
    fn compute_remapped_signature(&self, original: &str) -> DefaultAtom {
        let mut lock = self.signature_cache.write();
        let original_pooled_descriptor = DefaultAtom::from(original);
        lock.entry(PooledString(original_pooled_descriptor.clone()))
            .or_insert_with(|| {
                let parsed_original = MethodSignature::new(original).parse().unwrap();
                let remapped_descriptor = parsed_original
                    .remap_class(|original| {
                        self.try_get_class(original)
                            .map(|x| x.borrowed())
                            .unwrap_or(*original)
                    })
                    .descriptor();
                if remapped_descriptor == original {
                    // No need to intern again if they're equal
                    original_pooled_descriptor.clone()
                } else {
                    DefaultAtom::from(remapped_descriptor)
                }
            })
            .clone()
    }
}
impl fmt::Debug for MappingsSnapshot {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("Mappings")
            .field("classes", &self.classes)
            .field("methods", &self.methods)
            .field("fields", &self.fields)
            .finish()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn test_classes() -> Vec<(PooledJavaClass, PooledJavaClass)> {
        vec![
            (
                JavaClass::new("net/techcable/Example").intern(),
                JavaClass::new("com/example/Box").intern()
            ),
            (
                JavaClass::new("NotPackaged").intern(),
                JavaClass::new("com/example/Packaged").intern()
            ),
        ]
    }
    fn test_methods() -> Vec<(PooledMethodData, PooledMethodData)> {
        vec![
            (
                MethodData::parse_internal_name(
                    "net/techcable/Example/bob",
                    MethodSignature::new("(LNotPackaged;I)V"),
                ).unwrap()
                    .intern(),
                MethodData::parse_internal_name(
                    "com/example/Box/foo",
                    MethodSignature::new("(Lcom/example/Packaged;I)V"),
                ).unwrap()
                    .intern()
            ),
            (
                MethodData::parse_internal_name(
                    "net/techcable/Example/eat",
                    MethodSignature::new("(Lunchanged/ExampleClass;I)V"),
                ).unwrap()
                    .intern(),
                MethodData::parse_internal_name(
                    "com/example/Box/consume",
                    MethodSignature::new("(Lunchanged/ExampleClass;I)V"),
                ).unwrap()
                    .intern()
            ),
        ]
    }
    fn test_fields() -> Vec<(PooledFieldData, PooledFieldData)> {
        vec![
            (
                FieldData::parse_internal_name("unchanged/ExampleClass/foo")
                    .unwrap()
                    .intern(),
                FieldData::parse_internal_name("unchanged/ExampleClass/bar")
                    .unwrap()
                    .intern()
            ),
        ]
    }
    fn test_builder() -> MappingsBuilder {
        let mut builder = MappingsBuilder::new();
        for (original, renamed) in test_classes() {
            builder.insert_class(original, renamed);
        }
        for (original, renamed) in test_methods() {
            builder.insert_method(original, renamed.name);
        }
        for (original, renamed) in test_fields() {
            builder.insert_field(original, renamed.name);
        }
        builder
    }
    #[test]
    fn build_test() {
        let mut builder = test_builder();
        let result = builder.build();
        for (original, renamed) in test_classes() {
            assert_eq!(result.get_class(&original), renamed);
        }
        assert_eq!(
            result.get_class(&JavaClass::new("unchanged/ExampleClass")),
            JavaClass::new("unchanged/ExampleClass")
        );
        for (original, renamed) in test_methods() {
            assert_eq!(result.get_method(&original), renamed);
        }
        let implcitly_remapped = MethodData::parse_internal_name(
            "net/techcable/Example/implicit",
            MethodSignature::new("(ILNotPackaged;I)V"),
        ).unwrap();
        assert_eq!(
            result.get_method(&implcitly_remapped),
            MethodData::parse_internal_name(
                "com/example/Box/implicit",
                MethodSignature::new("(ILcom/example/Packaged;I)V"),
            ).unwrap()
        );
        for (original, renamed) in test_fields() {
            assert_eq!(result.get_field(&original), renamed);
        }
    }
    #[test]
    fn chain_test() {
        let mut original = test_builder();
        let mut chained = MappingsBuilder::new();
        chained.insert_class(
            JavaClass::new("com/example/Box").intern(),
            JavaClass::new("net/techcable/chained/ChainedBox").intern(),
        );
        chained.insert_method(
            MethodData::parse_internal_name(
                "com/example/Box/consume",
                MethodSignature::new("(Lunchanged/ExampleClass;I)V"),
            ).unwrap()
                .intern(),
            DefaultAtom::from("party"),
        );
        original.chain(&chained);
        let result = original.build();
        assert_eq!(
            result.get_method(&MethodData::parse_internal_name(
                "net/techcable/Example/eat",
                MethodSignature::new("(Lunchanged/ExampleClass;I)V"),
            ).unwrap()),
            MethodData::parse_internal_name(
                "net/techcable/chained/ChainedBox/party",
                MethodSignature::new("(Lunchanged/ExampleClass;I)V"),
            ).unwrap()
                .intern()
        )
    }
}
