//! The sizes of libexpat's C structures that the allocation tracker
//! charges (D-030, D-034), computed from the C definitions with the C
//! layout rules of the target.
//!
//! Each structure is written as its list of fields in declaration order,
//! each field as the `(size, alignment)` of its C type on the target being
//! compiled (`core::ffi` types, so `unsigned long` is 4 bytes on Windows
//! and 8 on 64-bit Unix). [`c_struct`] lays them out like a C compiler:
//! every field at the next multiple of its alignment, the structure padded
//! to a multiple of its largest alignment. The values are therefore not
//! hard-coded for one platform; `tests/tests/tracker_sizes.rs` compares
//! every one of them with `sizeof` / `offsetof` taken by the C compiler
//! from the vendored `xmlparse.c` (oracle) on every CI platform.
//!
//! Field lists are copied from libexpat 2.9.0 (`xmlparse.c`, `xmltok.h`,
//! `hash_table.h`; line numbers at each constant).

use core::ffi::{c_int, c_uchar, c_ulong};
use core::mem::{align_of, size_of};

/// `(size, alignment)` of a C type.
type Ty = (usize, usize);

/// A data pointer (`T *`).
const PTR: Ty = (size_of::<*const u8>(), align_of::<*const u8>());
/// `size_t`
const SIZE_T: Ty = (size_of::<usize>(), align_of::<usize>());
/// `int`
const INT: Ty = (size_of::<c_int>(), align_of::<c_int>());
/// `unsigned long`
const ULONG: Ty = (size_of::<c_ulong>(), align_of::<c_ulong>());
/// `unsigned char`, `char`, `XML_Bool` and `XML_Char` (D-002: one byte)
const UCHAR: Ty = (size_of::<c_uchar>(), align_of::<c_uchar>());
/// C99 `bool`
const BOOL: Ty = (size_of::<bool>(), align_of::<bool>());
/// A C `enum` whose constants fit an `int` (the oracle check confirms
/// that the CI compilers give it `int`'s layout).
const ENUM: Ty = INT;

/// Offset of the next field of type `t` after `end` bytes.
const fn align_up(end: usize, align: usize) -> usize {
    end.div_ceil(align) * align
}

/// `(size, alignment)` of a C structure with the given fields.
pub const fn c_struct(fields: &[Ty]) -> Ty {
    let mut end = 0;
    let mut align = 1;
    let mut rest = fields;
    while let [(s, a), tail @ ..] = rest {
        end = align_up(end, *a) + *s;
        if *a > align {
            align = *a;
        }
        rest = tail;
    }
    (align_up(end, align), align)
}

/// `offsetof` the member that follows `fields` when it has alignment
/// `next_align` (a flexible array member such as `BLOCK::s`).
const fn offset_after(fields: &[Ty], next_align: usize) -> usize {
    let mut end = 0;
    let mut rest = fields;
    while let [(s, a), tail @ ..] = rest {
        end = align_up(end, *a) + *s;
        rest = tail;
    }
    align_up(end, next_align)
}

/// `HASH_TABLE` (`hash_table.h` 48-54): `NAMED **v; unsigned char power;
/// size_t size; size_t used; XML_Parser parser;`
const HASH_TABLE: Ty = c_struct(&[PTR, UCHAR, SIZE_T, SIZE_T, PTR]);

/// `TAG_NAME` (`xmlparse.c` 277-284): three pointers, three `size_t`.
const TAG_NAME: Ty = c_struct(&[PTR, PTR, PTR, SIZE_T, SIZE_T, SIZE_T]);

/// `sizeof(TAG)` (`xmlparse.c` 299-310): `parent`, `rawName`,
/// `int rawNameLength`, `TAG_NAME name`, the `buf` union of two pointers,
/// `bufEnd`, `bindings`.
pub const TAG: usize = c_struct(&[PTR, PTR, INT, TAG_NAME, PTR, PTR, PTR]).0;

/// `sizeof(BINDING)` (`xmlparse.c` 262-270): five pointers, `size_t
/// uriLen`, `size_t uriAlloc`.
pub const BINDING: usize = c_struct(&[PTR, PTR, PTR, PTR, PTR, SIZE_T, SIZE_T]).0;

/// `sizeof(PREFIX)` (`xmlparse.c` 272-275): two pointers.
pub const PREFIX: usize = c_struct(&[PTR, PTR]).0;

/// `sizeof(ENTITY)` (`xmlparse.c` 312-328): two pointers, two `int`s,
/// four pointers, four `bool`s.
pub const ENTITY: usize = c_struct(&[PTR, PTR, INT, INT, PTR, PTR, PTR, PTR, BOOL, BOOL, BOOL, BOOL]).0;

/// `sizeof(CONTENT_SCAFFOLD)` (`xmlparse.c` 330-338): two enums, a
/// pointer, four `int`s.
pub const CONTENT_SCAFFOLD: usize = c_struct(&[ENUM, ENUM, PTR, INT, INT, INT, INT]).0;

/// `offsetof(BLOCK, s)` (`xmlparse.c` 342-346): `struct block *next; int
/// size;` before the `XML_Char` array.
pub const BLOCK_HEADER: usize = offset_after(&[PTR, INT], UCHAR.1);

/// `sizeof(ATTRIBUTE_ID)` (`xmlparse.c` 359-364): two pointers, two
/// `bool`s.
pub const ATTRIBUTE_ID: usize = c_struct(&[PTR, PTR, BOOL, BOOL]).0;

/// `sizeof(DEFAULT_ATTRIBUTE)` (`xmlparse.c` 366-370): a pointer, an
/// `XML_Bool`, a pointer.
pub const DEFAULT_ATTRIBUTE: usize = c_struct(&[PTR, UCHAR, PTR]).0;

/// `sizeof(NAME_AND_DEFAULT_ATTRIBUTE)` (`xmlparse.c` 374-386): a pointer,
/// `size_t attIndex`, `bool initialized`.
pub const NAME_AND_DEFAULT_ATTRIBUTE: usize = c_struct(&[PTR, SIZE_T, BOOL]).0;

/// `sizeof(NS_ATT)` (`xmlparse.c` 388-392): two `unsigned long`s and a
/// pointer.
pub const NS_ATT: usize = c_struct(&[ULONG, ULONG, PTR]).0;

/// `sizeof(ELEMENT_TYPE)` (`xmlparse.c` 394-402): three pointers, two
/// `size_t`, a pointer, a `HASH_TABLE`.
pub const ELEMENT_TYPE: usize = c_struct(&[PTR, PTR, PTR, SIZE_T, SIZE_T, PTR, HASH_TABLE]).0;

/// `sizeof(OPEN_INTERNAL_ENTITY)` (`xmlparse.c` 440-448): four pointers,
/// an `int`, a `bool`, an enum.
pub const OPEN_INTERNAL_ENTITY: usize = c_struct(&[PTR, PTR, PTR, PTR, INT, BOOL, ENUM]).0;

/// `sizeof(ATTRIBUTE)` (`xmltok.h` 152-157): three pointers and a `char`.
pub const ATTRIBUTE: usize = c_struct(&[PTR, PTR, PTR, UCHAR]).0;

/// `sizeof(NAMED *)`: one slot of a hash table.
pub const NAMED_PTR: usize = PTR.0;

/// `sizeof(size_t) + EXPAT_MALLOC_PADDING` (`internal.h` 117-124): the
/// header of a tracked allocation, `sizeof(union { long long l; void *p;
/// })`.
pub const MALLOC_HEADER: usize = c_union(&[
    (size_of::<core::ffi::c_longlong>(), align_of::<core::ffi::c_longlong>()),
    PTR,
]);

/// `sizeof` a C union of the given members.
const fn c_union(members: &[Ty]) -> usize {
    let mut size = 0;
    let mut align = 1;
    let mut rest = members;
    while let [(s, a), tail @ ..] = rest {
        if *s > size {
            size = *s;
        }
        if *a > align {
            align = *a;
        }
        rest = tail;
    }
    align_up(size, align)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_rules() {
        // Padding inside and at the end.
        assert_eq!(c_struct(&[(1, 1), (4, 4), (1, 1)]), (12, 4));
        assert_eq!(c_struct(&[(8, 8), (1, 1)]), (16, 8));
        assert_eq!(c_struct(&[]), (0, 1));
        assert_eq!(offset_after(&[(8, 8), (4, 4)], 1), 12);
    }

    #[cfg(all(target_pointer_width = "64", not(windows)))]
    #[test]
    fn lp64_values() {
        // Measured with the C compiler on 64-bit Linux (docs/DIVERGENCES.md
        // DV-1, R1); the integration test compares every platform.
        assert_eq!(
            [TAG, BINDING, NAME_AND_DEFAULT_ATTRIBUTE, ELEMENT_TYPE, ENTITY, NS_ATT, BLOCK_HEADER],
            [96, 56, 24, 88, 64, 24, 12]
        );
    }
}
