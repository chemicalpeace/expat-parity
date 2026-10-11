//! Canonical trace format (spec: `docs/ARCHITECTURE.md`, "Trace format").
//!
//! [`TraceWriter`] is the Rust mirror of `crates/expat-oracle/shim/oracle_trace.c`;
//! the two are kept honest by `tests/tests/trace_format.rs`, which checks the
//! writer and the C shim against the same hand-written expected traces.

use std::fmt::Write as _;

/// Event tags that may start a trace line.
pub const TAGS: [&str; 18] = [
    "xmldecl",
    "doctype",
    "enddoctype",
    "start",
    "attr",
    "end",
    "text",
    "pi",
    "comment",
    "cdata",
    "endcdata",
    "entity",
    "notation",
    "element",
    "attlist",
    "nsstart",
    "nsend",
    "skipped",
];

fn needs_escape(b: u8) -> bool {
    b <= 0x20 || b >= 0x7f || matches!(b, b'%' | b'~' | b'(' | b')' | b',')
}

/// Appends ` <field>` with the trace escaping rules; `None` is written as `~`.
pub fn push_field(out: &mut String, field: Option<&[u8]>) {
    out.push(' ');
    let Some(bytes) = field else {
        out.push('~');
        return;
    };
    for &b in bytes {
        if needs_escape(b) {
            // Writing to a String cannot fail.
            let _ = write!(out, "%{b:02X}");
        } else {
            out.push(char::from(b));
        }
    }
}

/// Status of the final `XML_Parse` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// `XML_STATUS_OK`
    Ok,
    /// `XML_STATUS_ERROR`
    Error,
}

/// Builds a canonical trace event by event, coalescing character data exactly
/// like the C shim.
#[derive(Debug, Default)]
pub struct TraceWriter {
    out: String,
    text: Vec<u8>,
    in_text: bool,
}

impl TraceWriter {
    /// Creates an empty writer.
    pub fn new() -> Self {
        Self::default()
    }

    fn flush_text(&mut self) {
        if self.in_text {
            self.out.push_str("text");
            let text = std::mem::take(&mut self.text);
            push_field(&mut self.out, Some(&text));
            self.out.push('\n');
            self.in_text = false;
        }
    }

    fn line(&mut self, tag: &str, fields: &[Option<&[u8]>], ints: &[i64]) {
        self.flush_text();
        self.out.push_str(tag);
        for f in fields {
            push_field(&mut self.out, *f);
        }
        for i in ints {
            let _ = write!(self.out, " {i}");
        }
        self.out.push('\n');
    }

    /// `XML_XmlDeclHandler`.
    pub fn xml_decl(&mut self, version: Option<&[u8]>, encoding: Option<&[u8]>, standalone: i32) {
        self.line("xmldecl", &[version, encoding], &[i64::from(standalone)]);
    }

    /// `XML_StartDoctypeDeclHandler`.
    pub fn doctype(
        &mut self,
        name: &[u8],
        sysid: Option<&[u8]>,
        pubid: Option<&[u8]>,
        has_internal_subset: bool,
    ) {
        self.line(
            "doctype",
            &[Some(name), sysid, pubid],
            &[i64::from(has_internal_subset)],
        );
    }

    /// `XML_EndDoctypeDeclHandler`.
    pub fn end_doctype(&mut self) {
        self.line("enddoctype", &[], &[]);
    }

    /// `XML_StartElementHandler` plus the attribute lines.
    pub fn start(&mut self, name: &[u8], specified: i32, attrs: &[(&[u8], &[u8])]) {
        self.line("start", &[Some(name)], &[i64::from(specified)]);
        for (n, v) in attrs {
            self.line("attr", &[Some(n), Some(v)], &[]);
        }
    }

    /// `XML_EndElementHandler`.
    pub fn end(&mut self, name: &[u8]) {
        self.line("end", &[Some(name)], &[]);
    }

    /// `XML_CharacterDataHandler` (coalesced).
    pub fn text(&mut self, data: &[u8]) {
        self.in_text = true;
        self.text.extend_from_slice(data);
    }

    /// `XML_ProcessingInstructionHandler`.
    pub fn pi(&mut self, target: &[u8], data: &[u8]) {
        self.line("pi", &[Some(target), Some(data)], &[]);
    }

    /// `XML_CommentHandler`.
    pub fn comment(&mut self, data: &[u8]) {
        self.line("comment", &[Some(data)], &[]);
    }

    /// `XML_StartCdataSectionHandler`.
    pub fn cdata(&mut self) {
        self.line("cdata", &[], &[]);
    }

    /// `XML_EndCdataSectionHandler`.
    pub fn end_cdata(&mut self) {
        self.line("endcdata", &[], &[]);
    }

    /// `XML_SkippedEntityHandler`.
    pub fn skipped(&mut self, name: &[u8], is_param: bool) {
        self.line("skipped", &[Some(name)], &[i64::from(is_param)]);
    }

    /// `XML_EntityDeclHandler` (`is_param` comes second, like the shim).
    #[allow(clippy::too_many_arguments)]
    pub fn entity(
        &mut self,
        name: &[u8],
        is_param: bool,
        value: Option<&[u8]>,
        base: Option<&[u8]>,
        sysid: Option<&[u8]>,
        pubid: Option<&[u8]>,
        notation: Option<&[u8]>,
    ) {
        self.flush_text();
        self.out.push_str("entity");
        push_field(&mut self.out, Some(name));
        let _ = write!(self.out, " {}", i32::from(is_param));
        for f in [value, base, sysid, pubid, notation] {
            push_field(&mut self.out, f);
        }
        self.out.push('\n');
    }

    /// `XML_NotationDeclHandler`.
    pub fn notation(
        &mut self,
        name: &[u8],
        base: Option<&[u8]>,
        sysid: Option<&[u8]>,
        pubid: Option<&[u8]>,
    ) {
        self.line("notation", &[Some(name), base, sysid, pubid], &[]);
    }

    /// `XML_AttlistDeclHandler`.
    pub fn attlist(
        &mut self,
        elname: &[u8],
        attname: &[u8],
        att_type: Option<&[u8]>,
        dflt: Option<&[u8]>,
        is_required: bool,
    ) {
        self.line(
            "attlist",
            &[Some(elname), Some(attname), att_type, dflt],
            &[i64::from(is_required)],
        );
    }

    /// `XML_ElementDeclHandler`: the name, then the model as
    /// `<type><quant> <name>(<child>,<child>...)`. Written without
    /// recursion, so any nesting depth is fine.
    pub fn element(&mut self, name: &[u8], model: &expat::internal::parse::ContentModel) {
        enum Item {
            Node(usize),
            Text(&'static str),
        }
        self.flush_text();
        self.out.push_str("element");
        push_field(&mut self.out, Some(name));
        self.out.push(' ');
        let mut stack = vec![Item::Node(0)];
        while let Some(item) = stack.pop() {
            match item {
                Item::Text(t) => self.out.push_str(t),
                Item::Node(i) => {
                    let Some(n) = model.nodes.get(i) else {
                        continue;
                    };
                    let _ = write!(self.out, "{}{}", n.kind as i32, n.quant as i32);
                    push_field(&mut self.out, n.name.as_deref());
                    self.out.push('(');
                    stack.push(Item::Text(")"));
                    let children: Vec<usize> = n.children.clone().collect();
                    for (k, c) in children.iter().enumerate().rev() {
                        stack.push(Item::Node(*c));
                        if k > 0 {
                            stack.push(Item::Text(","));
                        }
                    }
                }
            }
        }
        self.out.push('\n');
    }

    /// `XML_StartNamespaceDeclHandler`.
    pub fn ns_start(&mut self, prefix: Option<&[u8]>, uri: Option<&[u8]>) {
        self.line("nsstart", &[prefix, uri], &[]);
    }

    /// `XML_EndNamespaceDeclHandler`.
    pub fn ns_end(&mut self, prefix: Option<&[u8]>) {
        self.line("nsend", &[prefix], &[]);
    }

    /// Final line; consumes the writer and returns the trace bytes.
    pub fn finish(
        mut self,
        status: Status,
        error_code: u32,
        line: u64,
        column: u64,
        byte_index: i64,
    ) -> Vec<u8> {
        self.flush_text();
        let st = match status {
            Status::Ok => "ok",
            Status::Error => "error",
        };
        let _ = writeln!(
            self.out,
            "result {st} {error_code} {line} {column} {byte_index}"
        );
        self.out.into_bytes()
    }
}

/// True if every line starts with a known tag and the last line is a single
/// well-formed `result` line.
pub fn is_well_formed_trace(trace: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(trace) else {
        return false;
    };
    let Some(body) = text.strip_suffix('\n') else {
        return false;
    };
    let mut lines: Vec<&str> = body.split('\n').collect();
    let Some(last) = lines.pop() else {
        return false;
    };
    let result_ok = {
        let f: Vec<&str> = last.split(' ').collect();
        f.len() == 6
            && f.first() == Some(&"result")
            && matches!(f.get(1), Some(&"ok") | Some(&"error"))
            && f.iter().skip(2).all(|x| x.parse::<i64>().is_ok())
    };
    result_ok
        && lines.iter().all(|l| {
            let tag = l.split(' ').next().unwrap_or("");
            TAGS.contains(&tag) && !l.bytes().any(|b| b <= 0x1f || b >= 0x7f)
        })
}

/// The `result` line's error code, if the trace is well formed.
pub fn result_error_code(trace: &[u8]) -> Option<u32> {
    let text = std::str::from_utf8(trace).ok()?;
    let last = text.trim_end_matches('\n').rsplit('\n').next()?;
    last.split(' ').nth(2)?.parse().ok()
}

/// First differing line between two traces: `(1-based line, left, right)`.
pub fn first_difference(a: &[u8], b: &[u8]) -> Option<(usize, String, String)> {
    if a == b {
        return None;
    }
    let la: Vec<&[u8]> = a.split(|&c| c == b'\n').collect();
    let lb: Vec<&[u8]> = b.split(|&c| c == b'\n').collect();
    let n = la.len().max(lb.len());
    (0..n).find_map(|i| {
        let x = la.get(i).copied().unwrap_or(b"<missing>");
        let y = lb.get(i).copied().unwrap_or(b"<missing>");
        (x != y).then(|| {
            (
                i + 1,
                String::from_utf8_lossy(x).into_owned(),
                String::from_utf8_lossy(y).into_owned(),
            )
        })
    })
}
