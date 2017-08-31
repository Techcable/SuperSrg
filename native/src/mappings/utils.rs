use std::borrow::Cow;

use super::MappingsTransformer;
use types::{PooledJavaClass, JavaClass, JavaClassLookup};

pub struct PackageTransformer {
    original: String,
    renamed: String,
}
impl PackageTransformer {
    #[inline]
    pub fn single(original: String, mut renamed: String) -> Self {
        if original.is_empty() && !renamed.is_empty() {
            renamed.push('/');
        }
        PackageTransformer { original, renamed }
    }
}
impl MappingsTransformer for PackageTransformer {
    #[inline]
    fn transform_class<T: JavaClassLookup>(&self, original: &T) -> Option<Cow<PooledJavaClass>> {
        let original_name = original.internal_name();
        let original_package = &self.original;
        if original_name.is_char_boundary(original_package.len()) {
            let (first_part, remaining) = original_name.split_at(original_package.len());
            debug_assert_eq!(first_part.len(), original_package.len());
            if first_part == original_package {
                let mut buffer = String::with_capacity(self.renamed.len() + remaining.len());
                buffer.push_str(&self.renamed);
                buffer.push_str(remaining);
                return Some(Cow::Owned(JavaClass::new(&buffer).intern()));
            }
        }
        None
    }
}
