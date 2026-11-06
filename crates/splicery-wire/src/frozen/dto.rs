use std::io::{self, Write};

use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use splicery_domain::{CaptureGroup, FrozenSet, LineEnding, ResolvedCapture, SourceSpan};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FrozenDto {
    pub(super) schema_version: u64,
    pub(super) profile: String,
    pub(super) capture_sha256: String,
    pub(super) name: String,
    #[serde(default)]
    pub(super) scope: Option<SpanDto>,
    pub(super) members: Vec<MemberDto>,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SpanDto {
    pub(super) lo: usize,
    pub(super) hi: usize,
}

impl SpanDto {
    fn from_span(span: SourceSpan) -> Self {
        Self {
            lo: span.lo(),
            hi: span.hi(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MemberDto {
    pub(super) lo: usize,
    pub(super) hi: usize,
    #[serde(deserialize_with = "required_option")]
    pub(super) line: Option<LineDto>,
    pub(super) groups: Vec<GroupDto>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GroupDto {
    pub(super) name: String,
    #[serde(deserialize_with = "required_option")]
    pub(super) lo: Option<usize>,
    #[serde(deserialize_with = "required_option")]
    pub(super) hi: Option<usize>,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
pub(super) enum LineDto {
    Lf,
    CrLf,
    Unterminated,
}

impl LineDto {
    fn from_domain(line: LineEnding) -> Self {
        match line {
            LineEnding::Lf => Self::Lf,
            LineEnding::CrLf => Self::CrLf,
            LineEnding::Unterminated => Self::Unterminated,
        }
    }

    pub(super) fn into_domain(self) -> LineEnding {
        match self {
            Self::Lf => LineEnding::Lf,
            Self::CrLf => LineEnding::CrLf,
            Self::Unterminated => LineEnding::Unterminated,
        }
    }
}

fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

#[derive(Serialize)]
pub(super) struct FrozenRef<'a> {
    schema_version: u8,
    profile: &'a str,
    capture_sha256: String,
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<SpanDto>,
    members: MembersRef<'a>,
}

impl<'a> FrozenRef<'a> {
    pub(super) fn new(frozen: &'a FrozenSet) -> Self {
        Self {
            schema_version: 1,
            profile: &frozen.matcher_profile,
            capture_sha256: crate::hex(&frozen.capture_sha256),
            name: &frozen.name,
            scope: frozen.scope.map(SpanDto::from_span),
            members: MembersRef(&frozen.members),
        }
    }
}

struct MembersRef<'a>(&'a [ResolvedCapture]);

impl Serialize for MembersRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for member in self.0 {
            sequence.serialize_element(&MemberRef {
                lo: member.span.lo(),
                hi: member.span.hi(),
                line: member.line.map(LineDto::from_domain),
                groups: GroupsRef(&member.groups),
            })?;
        }
        sequence.end()
    }
}

#[derive(Serialize)]
struct MemberRef<'a> {
    lo: usize,
    hi: usize,
    line: Option<LineDto>,
    groups: GroupsRef<'a>,
}

struct GroupsRef<'a>(&'a [CaptureGroup]);

impl Serialize for GroupsRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for group in self.0 {
            sequence.serialize_element(&GroupRef {
                name: &group.name,
                lo: group.span.map(|span| span.lo()),
                hi: group.span.map(|span| span.hi()),
            })?;
        }
        sequence.end()
    }
}

#[derive(Serialize)]
struct GroupRef<'a> {
    name: &'a str,
    lo: Option<usize>,
    hi: Option<usize>,
}

struct BoundedWriter {
    bytes: Vec<u8>,
    remaining: usize,
}

impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.remaining {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "JSON bound exceeded",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        self.remaining -= bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn bounded_json<T: Serialize>(value: &T) -> Result<String, String> {
    let mut writer = BoundedWriter {
        bytes: Vec::new(),
        remaining: super::MAX_FROZEN_BYTES,
    };
    serde_json::to_writer(&mut writer, value)
        .map_err(|error| format!("cannot encode bounded frozen JSON: {error}"))?;
    String::from_utf8(writer.bytes).map_err(|_| "encoded frozen JSON is not UTF-8".to_string())
}
