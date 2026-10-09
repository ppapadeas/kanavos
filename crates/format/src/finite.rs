//! A serializer that writes nothing and fails on the first non-finite float, reporting where it is.
//!
//! JSON has no representation for NaN or infinity; `serde_json` silently writes `null` instead, which would make
//! the saved file unreadable. Running this check before writing turns that into a clear error that names the
//! offending field.

use std::fmt;

use serde::Serialize;
use serde::ser::{self, Impossible};

/// One step of a path into the document.
#[derive(Debug, Clone)]
enum Seg {
    Field(&'static str),
    Key(String),
    Index(usize),
}

/// Error raised by the checker: either a non-finite number at `path`, or a custom serialisation error.
#[derive(Debug)]
pub(crate) struct CheckError {
    pub(crate) path: String,
    pub(crate) custom: Option<String>,
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.custom {
            Some(m) => write!(f, "{m}"),
            None => write!(f, "non-finite number at {}", self.path),
        }
    }
}

impl std::error::Error for CheckError {}

impl ser::Error for CheckError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        CheckError { path: String::new(), custom: Some(msg.to_string()) }
    }
}

fn render(path: &[Seg]) -> String {
    let mut out = String::new();
    for (i, s) in path.iter().enumerate() {
        if i > 0 {
            out.push('.');
        }
        match s {
            Seg::Field(f) => out.push_str(f),
            Seg::Key(k) => out.push_str(k),
            Seg::Index(n) => out.push_str(&n.to_string()),
        }
    }
    if out.is_empty() { "<root>".to_owned() } else { out }
}

/// Checks that every float in `value` is finite.
pub(crate) fn check<T: Serialize + ?Sized>(value: &T) -> Result<(), CheckError> {
    let mut path = Vec::new();
    value.serialize(Checker { path: &mut path })
}

struct Checker<'a> {
    path: &'a mut Vec<Seg>,
}

impl Checker<'_> {
    fn float(self, v: f64) -> Result<(), CheckError> {
        if v.is_finite() { Ok(()) } else { Err(CheckError { path: render(self.path), custom: None }) }
    }
}

struct Compound<'a> {
    path: &'a mut Vec<Seg>,
    index: usize,
    key: Option<String>,
}

impl Compound<'_> {
    fn nested<T: Serialize + ?Sized>(&mut self, seg: Seg, value: &T) -> Result<(), CheckError> {
        self.path.push(seg);
        let r = value.serialize(Checker { path: self.path });
        self.path.pop();
        r
    }
}

impl<'a> ser::Serializer for Checker<'a> {
    type Ok = ();
    type Error = CheckError;
    type SerializeSeq = Compound<'a>;
    type SerializeTuple = Compound<'a>;
    type SerializeTupleStruct = Compound<'a>;
    type SerializeTupleVariant = Compound<'a>;
    type SerializeMap = Compound<'a>;
    type SerializeStruct = Compound<'a>;
    type SerializeStructVariant = Compound<'a>;

    fn serialize_bool(self, _: bool) -> Result<(), CheckError> {
        Ok(())
    }
    fn serialize_i8(self, _: i8) -> Result<(), CheckError> {
        Ok(())
    }
    fn serialize_i16(self, _: i16) -> Result<(), CheckError> {
        Ok(())
    }
    fn serialize_i32(self, _: i32) -> Result<(), CheckError> {
        Ok(())
    }
    fn serialize_i64(self, _: i64) -> Result<(), CheckError> {
        Ok(())
    }
    fn serialize_u8(self, _: u8) -> Result<(), CheckError> {
        Ok(())
    }
    fn serialize_u16(self, _: u16) -> Result<(), CheckError> {
        Ok(())
    }
    fn serialize_u32(self, _: u32) -> Result<(), CheckError> {
        Ok(())
    }
    fn serialize_u64(self, _: u64) -> Result<(), CheckError> {
        Ok(())
    }
    fn serialize_f32(self, v: f32) -> Result<(), CheckError> {
        self.float(f64::from(v))
    }
    fn serialize_f64(self, v: f64) -> Result<(), CheckError> {
        self.float(v)
    }
    fn serialize_char(self, _: char) -> Result<(), CheckError> {
        Ok(())
    }
    fn serialize_str(self, _: &str) -> Result<(), CheckError> {
        Ok(())
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<(), CheckError> {
        Ok(())
    }
    fn serialize_none(self) -> Result<(), CheckError> {
        Ok(())
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), CheckError> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<(), CheckError> {
        Ok(())
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), CheckError> {
        Ok(())
    }
    fn serialize_unit_variant(self, _: &'static str, _: u32, _: &'static str) -> Result<(), CheckError> {
        Ok(())
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<(), CheckError> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<(), CheckError> {
        self.path.push(Seg::Field(variant));
        let r = value.serialize(Checker { path: self.path });
        self.path.pop();
        r
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Compound<'a>, CheckError> {
        Ok(Compound { path: self.path, index: 0, key: None })
    }
    fn serialize_tuple(self, _: usize) -> Result<Compound<'a>, CheckError> {
        Ok(Compound { path: self.path, index: 0, key: None })
    }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Compound<'a>, CheckError> {
        Ok(Compound { path: self.path, index: 0, key: None })
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Compound<'a>, CheckError> {
        self.path.push(Seg::Field(variant));
        Ok(Compound { path: self.path, index: 0, key: None })
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Compound<'a>, CheckError> {
        Ok(Compound { path: self.path, index: 0, key: None })
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Compound<'a>, CheckError> {
        Ok(Compound { path: self.path, index: 0, key: None })
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Compound<'a>, CheckError> {
        self.path.push(Seg::Field(variant));
        Ok(Compound { path: self.path, index: 0, key: None })
    }
}

impl ser::SerializeSeq for Compound<'_> {
    type Ok = ();
    type Error = CheckError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CheckError> {
        let i = self.index;
        self.index += 1;
        self.nested(Seg::Index(i), value)
    }
    fn end(self) -> Result<(), CheckError> {
        Ok(())
    }
}

impl ser::SerializeTuple for Compound<'_> {
    type Ok = ();
    type Error = CheckError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CheckError> {
        ser::SerializeSeq::serialize_element(self, value)
    }
    fn end(self) -> Result<(), CheckError> {
        Ok(())
    }
}

impl ser::SerializeTupleStruct for Compound<'_> {
    type Ok = ();
    type Error = CheckError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CheckError> {
        ser::SerializeSeq::serialize_element(self, value)
    }
    fn end(self) -> Result<(), CheckError> {
        Ok(())
    }
}

impl ser::SerializeTupleVariant for Compound<'_> {
    type Ok = ();
    type Error = CheckError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CheckError> {
        ser::SerializeSeq::serialize_element(self, value)
    }
    fn end(self) -> Result<(), CheckError> {
        self.path.pop();
        Ok(())
    }
}

impl ser::SerializeMap for Compound<'_> {
    type Ok = ();
    type Error = CheckError;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), CheckError> {
        self.key = Some(key.serialize(KeyName).unwrap_or_else(|_| "?".to_owned()));
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CheckError> {
        let key = self.key.take().unwrap_or_else(|| "?".to_owned());
        self.nested(Seg::Key(key), value)
    }
    fn end(self) -> Result<(), CheckError> {
        Ok(())
    }
}

impl ser::SerializeStruct for Compound<'_> {
    type Ok = ();
    type Error = CheckError;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), CheckError> {
        self.nested(Seg::Field(key), value)
    }
    fn end(self) -> Result<(), CheckError> {
        Ok(())
    }
}

impl ser::SerializeStructVariant for Compound<'_> {
    type Ok = ();
    type Error = CheckError;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), CheckError> {
        self.nested(Seg::Field(key), value)
    }
    fn end(self) -> Result<(), CheckError> {
        self.path.pop();
        Ok(())
    }
}

/// Renders a map key (a string or a number) as text for error paths.
struct KeyName;

impl ser::Serializer for KeyName {
    type Ok = String;
    type Error = CheckError;
    type SerializeSeq = Impossible<String, CheckError>;
    type SerializeTuple = Impossible<String, CheckError>;
    type SerializeTupleStruct = Impossible<String, CheckError>;
    type SerializeTupleVariant = Impossible<String, CheckError>;
    type SerializeMap = Impossible<String, CheckError>;
    type SerializeStruct = Impossible<String, CheckError>;
    type SerializeStructVariant = Impossible<String, CheckError>;

    fn serialize_bool(self, v: bool) -> Result<String, CheckError> {
        Ok(v.to_string())
    }
    fn serialize_i8(self, v: i8) -> Result<String, CheckError> {
        Ok(v.to_string())
    }
    fn serialize_i16(self, v: i16) -> Result<String, CheckError> {
        Ok(v.to_string())
    }
    fn serialize_i32(self, v: i32) -> Result<String, CheckError> {
        Ok(v.to_string())
    }
    fn serialize_i64(self, v: i64) -> Result<String, CheckError> {
        Ok(v.to_string())
    }
    fn serialize_u8(self, v: u8) -> Result<String, CheckError> {
        Ok(v.to_string())
    }
    fn serialize_u16(self, v: u16) -> Result<String, CheckError> {
        Ok(v.to_string())
    }
    fn serialize_u32(self, v: u32) -> Result<String, CheckError> {
        Ok(v.to_string())
    }
    fn serialize_u64(self, v: u64) -> Result<String, CheckError> {
        Ok(v.to_string())
    }
    fn serialize_f32(self, v: f32) -> Result<String, CheckError> {
        Ok(v.to_string())
    }
    fn serialize_f64(self, v: f64) -> Result<String, CheckError> {
        Ok(v.to_string())
    }
    fn serialize_char(self, v: char) -> Result<String, CheckError> {
        Ok(v.to_string())
    }
    fn serialize_str(self, v: &str) -> Result<String, CheckError> {
        Ok(v.to_owned())
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<String, CheckError> {
        Ok("<bytes>".to_owned())
    }
    fn serialize_none(self) -> Result<String, CheckError> {
        Ok("null".to_owned())
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<String, CheckError> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<String, CheckError> {
        Ok("()".to_owned())
    }
    fn serialize_unit_struct(self, name: &'static str) -> Result<String, CheckError> {
        Ok(name.to_owned())
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<String, CheckError> {
        Ok(variant.to_owned())
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<String, CheckError> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: &T,
    ) -> Result<String, CheckError> {
        Ok(variant.to_owned())
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, CheckError> {
        Err(ser::Error::custom("complex map key"))
    }
    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, CheckError> {
        Err(ser::Error::custom("complex map key"))
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, CheckError> {
        Err(ser::Error::custom("complex map key"))
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, CheckError> {
        Err(ser::Error::custom("complex map key"))
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, CheckError> {
        Err(ser::Error::custom("complex map key"))
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self::SerializeStruct, CheckError> {
        Err(ser::Error::custom("complex map key"))
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, CheckError> {
        Err(ser::Error::custom("complex map key"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[derive(Serialize)]
    struct Inner {
        a: f64,
        list: Vec<f64>,
    }

    #[derive(Serialize)]
    #[serde(tag = "type")]
    enum Tagged {
        Thing { inner: Inner },
    }

    #[test]
    fn finite_values_pass() {
        let mut m = BTreeMap::new();
        m.insert(7_u64, Inner { a: 1.0, list: vec![2.0, -0.0] });
        assert!(check(&m).is_ok());
    }

    #[test]
    fn reports_the_path_of_a_nan() {
        let mut m = BTreeMap::new();
        m.insert(7_u64, Tagged::Thing { inner: Inner { a: 1.0, list: vec![2.0, f64::NAN] } });
        let err = check(&m).unwrap_err();
        assert_eq!(err.path, "7.inner.list.1");
    }

    #[test]
    fn reports_infinity_in_a_struct_field() {
        let err = check(&Inner { a: f64::INFINITY, list: vec![] }).unwrap_err();
        assert_eq!(err.path, "a");
    }
}
