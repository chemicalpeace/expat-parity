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

use core::ffi::{c_float, c_int, c_uchar, c_uint, c_ulong, c_ulonglong};
use core::mem::{align_of, size_of};

/// `(size, alignment)` of a C type.
type Ty = (usize, usize);

/// A data pointer (`T *`).
const PTR: Ty = (size_of::<*const u8>(), align_of::<*const u8>());
/// `size_t`
const SIZE_T: Ty = (size_of::<usize>(), align_of::<usize>());
/// `int`
const INT: Ty = (size_of::<c_int>(), align_of::<c_int>());
/// `unsigned long` (4 bytes on Windows, 8 on 64-bit Unix)
const ULONG: Ty = (size_of::<c_ulong>(), align_of::<c_ulong>());
/// `unsigned` / `unsigned int`
const UINT: Ty = (size_of::<c_uint>(), align_of::<c_uint>());
/// `uint64_t` and `unsigned long long` (`XmlBigCount`)
const U64: Ty = (size_of::<c_ulonglong>(), align_of::<c_ulonglong>());
/// `float`
const FLOAT: Ty = (size_of::<c_float>(), align_of::<c_float>());
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

/// `STRING_POOL` (`xmlparse.c` 348-355): five pointers and the parser.
const STRING_POOL: Ty = c_struct(&[PTR, PTR, PTR, PTR, PTR, PTR]);

/// `INIT_ENCODING` (`xmltok.h` 172-199, 280-283): an `ENCODING` (four
/// scanners, `XML_N_STATES` with `XML_DTD`, two literal scanners, ten
/// function pointers, `int minBytesPerChar`, `char isUtf8`, `char
/// isUtf16`) and `const ENCODING **encPtr`.
const INIT_ENCODING: Ty = c_struct(&[
    c_struct(&[
        PTR, PTR, PTR, PTR, PTR, PTR, PTR, PTR, PTR, PTR, PTR, PTR, PTR, PTR, PTR, PTR, INT, UCHAR, UCHAR,
    ]),
    PTR,
]);

/// `PROLOG_STATE` (`xmlrole.h` 110-120, with `XML_DTD`): the handler,
/// `unsigned level`, `int role_none`, `unsigned includeLevel`, `int
/// documentEntity`, `int inEntityValue`.
const PROLOG_STATE: Ty = c_struct(&[PTR, UINT, INT, UINT, INT, INT]);

/// `ACCOUNTING` (`xmlparse.c` 459-465): two `XmlBigCount`s, `unsigned
/// long debugLevel`, `float`, `unsigned long long`.
const fn accounting(ulong: Ty) -> Ty {
    c_struct(&[U64, U64, ulong, FLOAT, U64])
}

/// `MALLOC_TRACKER` (`xmlparse.c` 467-473): two `size_t`, `unsigned long
/// debugLevel`, `float`, `XmlBigCount`.
const fn malloc_tracker(ulong: Ty) -> Ty {
    c_struct(&[SIZE_T, SIZE_T, ulong, FLOAT, U64])
}

/// `ENTITY_STATS` (`xmlparse.c` 475-480): three `unsigned int`s and
/// `unsigned long debugLevel`.
const fn entity_stats(ulong: Ty) -> Ty {
    c_struct(&[UINT, UINT, UINT, ulong])
}

/// `struct XML_ParserStruct` (`xmlparse.c` 711-832, with `XML_DTD` and
/// `XML_GE == 1`) for a target whose `unsigned long` is `ulong`. Its four
/// `unsigned long` members (`m_nsAttsVersion` and the `debugLevel` of
/// `m_accounting`, `m_alloc_tracker` and `m_entity_stats`) are the only
/// ones whose size differs between 64-bit Unix and 64-bit Windows.
pub(crate) const fn parser_struct(ulong: Ty) -> Ty {
    c_struct(&[
        PTR,                              // m_userData
        PTR,                              // m_handlerArg
        PTR,                              // m_buffer
        c_struct(&[PTR, PTR, PTR]),       // m_mem (XML_Memory_Handling_Suite)
        PTR,                              // m_bufferPtr
        PTR,                              // m_bufferEnd
        PTR,                              // m_bufferLim
        U64,                              // m_parseEndByteIndex
        PTR,                              // m_parseEndPtr
        SIZE_T,                           // m_partialTokenBytesBefore
        UCHAR,                            // m_reparseDeferralEnabled
        INT,                              // m_lastBufferRequestSize
        PTR,                              // m_dataBuf
        PTR,                              // m_dataBufEnd
        PTR,                              // m_startElementHandler
        PTR,                              // m_endElementHandler
        PTR,                              // m_characterDataHandler
        PTR,                              // m_processingInstructionHandler
        PTR,                              // m_commentHandler
        PTR,                              // m_startCdataSectionHandler
        PTR,                              // m_endCdataSectionHandler
        PTR,                              // m_defaultHandler
        PTR,                              // m_startDoctypeDeclHandler
        PTR,                              // m_endDoctypeDeclHandler
        PTR,                              // m_unparsedEntityDeclHandler
        PTR,                              // m_notationDeclHandler
        PTR,                              // m_startNamespaceDeclHandler
        PTR,                              // m_endNamespaceDeclHandler
        PTR,                              // m_notStandaloneHandler
        PTR,                              // m_externalEntityRefHandler
        PTR,                              // m_externalEntityRefHandlerArg
        PTR,                              // m_skippedEntityHandler
        PTR,                              // m_unknownEncodingHandler
        PTR,                              // m_elementDeclHandler
        PTR,                              // m_attlistDeclHandler
        PTR,                              // m_entityDeclHandler
        PTR,                              // m_xmlDeclHandler
        PTR,                              // m_encoding
        INIT_ENCODING,                    // m_initEncoding
        PTR,                              // m_internalEncoding
        PTR,                              // m_protocolEncodingName
        UCHAR,                            // m_ns
        UCHAR,                            // m_ns_triplets
        PTR,                              // m_unknownEncodingMem
        PTR,                              // m_unknownEncodingData
        PTR,                              // m_unknownEncodingHandlerData
        PTR,                              // m_unknownEncodingConvert
        PTR,                              // m_unknownEncodingRelease
        PROLOG_STATE,                     // m_prologState
        PTR,                              // m_processor
        ENUM,                             // m_errorCode
        PTR,                              // m_eventPtr
        PTR,                              // m_eventEndPtr
        PTR,                              // m_positionPtr
        PTR,                              // m_openInternalEntities
        PTR,                              // m_openAttributeEntities
        PTR,                              // m_openValueEntities
        PTR,                              // m_freeEntities
        UCHAR,                            // m_defaultExpandInternalEntities
        INT,                              // m_tagLevel
        PTR,                              // m_declEntity
        PTR,                              // m_doctypeName
        PTR,                              // m_doctypeSysid
        PTR,                              // m_doctypePubid
        PTR,                              // m_declAttributeType
        PTR,                              // m_declNotationName
        PTR,                              // m_declNotationPublicId
        PTR,                              // m_declElementType
        PTR,                              // m_declAttributeId
        UCHAR,                            // m_declAttributeIsCdata
        UCHAR,                            // m_declAttributeIsId
        PTR,                              // m_dtd
        PTR,                              // m_curBase
        PTR,                              // m_tagStack
        PTR,                              // m_freeTagList
        PTR,                              // m_inheritedBindings
        PTR,                              // m_freeBindingList
        SIZE_T,                           // m_attsSize
        INT,                              // m_nSpecifiedAtts
        INT,                              // m_idAttIndex
        PTR,                              // m_atts
        PTR,                              // m_nsAtts
        ulong,                            // m_nsAttsVersion
        UCHAR,                            // m_nsAttsPower
        c_struct(&[U64, U64]),            // m_position (POSITION)
        STRING_POOL,                      // m_tempPool
        STRING_POOL,                      // m_temp2Pool
        PTR,                              // m_groupConnector
        SIZE_T,                           // m_groupSize
        UCHAR,                            // m_namespaceSeparator
        PTR,                              // m_parentParser
        c_struct(&[ENUM, UCHAR]),         // m_parsingStatus (XML_ParsingStatus)
        UCHAR,                            // m_isParamEntity
        UCHAR,                            // m_useForeignDTD
        ENUM,                             // m_paramEntityParsing
        c_struct(&[U64, U64]),            // m_hash_secret_salt_128 (struct sipkey)
        UCHAR,                            // m_hash_secret_salt_set
        accounting(ulong),                // m_accounting
        malloc_tracker(ulong),            // m_alloc_tracker
        entity_stats(ulong),              // m_entity_stats
        UCHAR,                            // m_reenter
        UINT,                             // m_handlerCallDepth
    ])
}

/// `sizeof(struct XML_ParserStruct)`: 1,104 bytes on 64-bit Unix, 1,072
/// on 64-bit Windows (the four `unsigned long` members, 8 bytes each).
pub const PARSER: usize = parser_struct(ULONG).0;

/// `offsetof(struct XML_ParserStruct, m_position)`: the members up to
/// `m_nsAttsVersion` and `m_nsAttsPower`, padded to `POSITION`'s
/// alignment.
pub const PARSER_M_POSITION: usize = parser_m_position(ULONG);

const fn parser_m_position(ulong: Ty) -> usize {
    // The parser structure's members before `m_position`, with the same
    // list as `parser_struct` (checked by the layout test).
    let full = parser_struct(ulong).0;
    let after = c_struct(&[
        c_struct(&[U64, U64]),
        STRING_POOL,
        STRING_POOL,
        PTR,
        SIZE_T,
        UCHAR,
        PTR,
        c_struct(&[ENUM, UCHAR]),
        UCHAR,
        UCHAR,
        ENUM,
        c_struct(&[U64, U64]),
        UCHAR,
        accounting(ulong),
        malloc_tracker(ulong),
        entity_stats(ulong),
        UCHAR,
        UINT,
    ])
    .0;
    full - after
}

/// `sizeof(ACCOUNTING)`.
pub const ACCOUNTING: usize = accounting(ULONG).0;
/// `sizeof(MALLOC_TRACKER)`.
pub const MALLOC_TRACKER: usize = malloc_tracker(ULONG).0;
/// `sizeof(ENTITY_STATS)`.
pub const ENTITY_STATS: usize = entity_stats(ULONG).0;

/// `sizeof(DTD)` (`xmlparse.c` 404-432, with `XML_DTD`): four hash
/// tables, two string pools, three `XML_Bool`s, `bool paramEntityRead`,
/// the parameter entity table, `PREFIX defaultPrefix`, `bool in_eldecl`,
/// the scaffold pointer, three `unsigned`, `int scaffLevel`, `int
/// *scaffIndex`, `size_t scaffIndexSize`.
pub const DTD: usize = c_struct(&[
    HASH_TABLE,
    HASH_TABLE,
    HASH_TABLE,
    HASH_TABLE,
    STRING_POOL,
    STRING_POOL,
    UCHAR,
    UCHAR,
    UCHAR,
    BOOL,
    HASH_TABLE,
    c_struct(&[PTR, PTR]),
    BOOL,
    PTR,
    UINT,
    UINT,
    UINT,
    INT,
    PTR,
    SIZE_T,
])
.0;

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
        assert_eq!([PARSER, DTD, PARSER_M_POSITION], [1104, 368, 808]);
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn parser_struct_with_a_four_byte_unsigned_long() {
        // The layout a 64-bit Windows compiler gives the parser structure
        // (`unsigned long` 4 bytes, aligned to 4): 32 bytes less than on
        // 64-bit Unix, 8 for each `unsigned long` member: m_nsAttsVersion
        // (with m_nsAttsPower before an 8-aligned POSITION), and the
        // debugLevel of ACCOUNTING (40 -> 32), MALLOC_TRACKER (40 -> 32)
        // and ENTITY_STATS (24 -> 16, the rest of the structure packing
        // into the freed padding). Measured with clang's MSVC target and
        // confirmed against MSVC by tests/tests/tracker_sizes.rs on CI.
        assert_eq!(parser_struct((4, 4)).0, 1072);
        assert_eq!(parser_struct((8, 8)).0, 1104);
        assert_eq!(parser_m_position((4, 4)), 800);
        assert_eq!(accounting((4, 4)).0, 32);
        assert_eq!(malloc_tracker((4, 4)).0, 32);
        assert_eq!(entity_stats((4, 4)).0, 16);
    }
}
