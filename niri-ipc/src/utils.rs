pub(crate) mod one_or_many {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub(crate) fn serialize<T: Serialize, S: Serializer>(
        value: &Vec<T>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        if value.len() == 1 {
            value[0].serialize(serializer)
        } else {
            value.serialize(serializer)
        }
    }

    pub(crate) fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<T>, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum OneOrMany<T> {
            Many(Vec<T>),
            One(T),
        }

        match OneOrMany::deserialize(deserializer)? {
            OneOrMany::Many(v) => Ok(v),
            OneOrMany::One(v) => Ok(vec![v]),
        }
    }

    #[cfg(test)]
    mod tests {
        use serde_json::de::SliceRead;
        use serde_json::{Deserializer, Serializer, Value};

        use super::*;

        #[test]
        fn serialize_one() {
            let mut result = Vec::new();
            let mut serializer = Serializer::new(&mut result);
            serialize(&vec![Value::Null], &mut serializer).expect("failed to serialize");
            assert_eq!(String::from_utf8_lossy(&result), "null");
        }

        #[test]
        fn deserialize_one() {
            let mut deserailier = Deserializer::new(SliceRead::new("null".as_bytes()));
            let result: Vec<Value> = deserialize(&mut deserailier).expect("failed to deserialize");
            assert_eq!(result, vec![Value::Null]);
        }

        #[test]
        fn serialize_many() {
            let mut result = Vec::new();
            let mut serializer = Serializer::new(&mut result);
            serialize(&vec![Value::Null, Value::Null], &mut serializer)
                .expect("failed to serialize");
            assert_eq!(String::from_utf8_lossy(&result), "[null,null]");
        }

        #[test]
        fn deserialize_many() {
            let mut deserailier = Deserializer::new(SliceRead::new("[null,null]".as_bytes()));
            let result: Vec<Value> = deserialize(&mut deserailier).expect("failed to deserialize");
            assert_eq!(result, vec![Value::Null, Value::Null]);
        }

        #[test]
        fn serialize_none() {
            let mut result = Vec::new();
            let mut serializer = Serializer::new(&mut result);
            serialize(&Vec::<Value>::new(), &mut serializer).expect("failed to serialize");
            assert_eq!(String::from_utf8_lossy(&result), "[]");
        }

        #[test]
        fn deserialize_none() {
            let mut deserailier = Deserializer::new(SliceRead::new("[]".as_bytes()));
            let result: Vec<Value> = deserialize(&mut deserailier).expect("failed to deserialize");
            assert_eq!(result, Vec::<Value>::new());
        }

        #[test]
        fn serialize_derive() {
            #[derive(Debug, Serialize, PartialEq)]
            enum Request {
                Action(#[serde(with = "self")] Vec<String>),
            }
            let request = serde_json::to_string(&Request::Action(vec!["foo".to_string()]))
                .expect("failed to serialize");
            assert_eq!(request, r#"{"Action":"foo"}"#);
            let request =
                serde_json::to_string(&Request::Action(vec!["foo".to_string(), "bar".to_string()]))
                    .expect("failed to serialize");
            assert_eq!(request, r#"{"Action":["foo","bar"]}"#);
        }

        #[test]
        fn deserialize_derive() {
            #[derive(Debug, Deserialize, PartialEq)]
            enum Request {
                Action(#[serde(with = "self")] Vec<String>),
            }
            let request: Request =
                serde_json::from_str(r#"{"Action":"foo"}"#).expect("failed to deserialize");
            assert_eq!(request, Request::Action(vec!["foo".to_string()]));
            let request: Request =
                serde_json::from_str(r#"{"Action":["foo","bar"]}"#).expect("failed to deserialize");
            assert_eq!(
                request,
                Request::Action(vec!["foo".to_string(), "bar".to_string()])
            );
        }
    }
}
