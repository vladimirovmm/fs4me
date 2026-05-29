use std::{
    collections::HashMap,
    ffi::CString,
    fmt::{Debug, Display},
};

/// Параметры для подключения к хранилищу файлов.
#[derive(Default, Debug)]
pub struct DriverParams(pub HashMap<String, String>);

/// Разбор строки в формате `KEY=VALUE\nKEY=VALUE` в параметры подключения.
impl From<&str> for DriverParams {
    fn from(value: &str) -> Self {
        let params = value
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .filter_map(|line: &str| line.split_once("="))
            .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
            .collect::<HashMap<_, _>>();
        DriverParams(params)
    }
}

/// Разбор CString в формате `KEY=VALUE\nKEY=VALUE` в параметры подключения.
impl From<CString> for DriverParams {
    fn from(value: CString) -> Self {
        value.to_str().map(Into::into).unwrap_or_default()
    }
}

/// Разбор строки в формате `KEY=VALUE\nKEY=VALUE` в параметры подключения.
impl From<String> for DriverParams {
    fn from(value: String) -> Self {
        value.as_str().into()
    }
}

impl Display for DriverParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (key, value) in &self.0 {
            writeln!(f, "{key}={value}")?;
        }
        Ok(())
    }
}

#[test]
fn test_open_params() {
    let examples = vec![
        ("", vec![]),
        (" ", vec![]),
        ("a=1", vec![("a", "1")]),
        ("a=1\nb=2", vec![("a", "1"), ("b", "2")]),
        ("a=1\nb=2\nc=3", vec![("a", "1"), ("b", "2"), ("c", "3")]),
        (
            "a=1\n     \n\n\n    \nb=2\nc=3\n",
            vec![("a", "1"), ("b", "2"), ("c", "3")],
        ),
    ];
    for (input, expected) in examples {
        let actual = DriverParams::from(input);
        assert_eq!(actual.0.len(), expected.len());

        for (expected_key, expected_value) in expected {
            let actual_value: Option<&str> = actual.0.get(expected_key).map(|v| v.as_str());
            assert_eq!(actual_value, Some(expected_value));
        }
    }
}
