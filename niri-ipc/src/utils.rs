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
        use std::fmt::Debug;

        use serde_json::de::SliceRead;
        use serde_json::{Deserializer, Serializer, Value};

        use super::*;

        fn test_serialize<T: Serialize>(value: &Vec<T>, expected: &str) {
            let mut bytes = Vec::new();
            let mut serializer = Serializer::new(&mut bytes);
            serialize(value, &mut serializer).expect("failed to serialize");
            assert_eq!(String::from_utf8_lossy(&bytes), expected);
        }

        fn test_deserialize<'de, T>(value: &'de str, expected: &Vec<T>)
        where
            T: Deserialize<'de> + Debug + PartialEq,
        {
            let mut deserailier = Deserializer::new(SliceRead::new(value.as_bytes()));
            let result: Vec<T> = deserialize(&mut deserailier).expect("failed to deserialize");
            assert_eq!(&result, expected);
        }

        #[test]
        fn serialize_one() {
            test_serialize(&vec![Value::Null], "null");
        }

        #[test]
        fn deserialize_one() {
            test_deserialize("null", &vec![Value::Null]);
        }

        #[test]
        fn serialize_many() {
            test_serialize(&vec![Value::Null, Value::Null], "[null,null]");
        }

        #[test]
        fn deserialize_many() {
            test_deserialize("[null,null]", &vec![Value::Null, Value::Null]);
        }

        #[test]
        fn serialize_none() {
            test_serialize(&Vec::<Value>::new(), "[]");
        }

        #[test]
        fn deserialize_none() {
            test_deserialize("[]", &Vec::<Value>::new());
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
