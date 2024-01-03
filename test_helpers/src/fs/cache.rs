use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use serde::Serialize;
use serde_json::Value;
use serde_with::skip_serializing_none;

use crate::TestContext;

#[skip_serializing_none]
#[derive(Clone, Serialize)]
struct RootSchemaEntry {
    list_endpoint: String,
    schema: String,
}

#[derive(Clone)]
pub struct RootSchemaBuilder {
    api_path: String,
    base_path: PathBuf,
    root_schema: HashMap<String, RootSchemaEntry>,
}

impl RootSchemaBuilder {
    pub(crate) fn new(test_context: &TestContext, api_path: impl Into<String>) -> Self {
        Self {
            api_path: api_path.into(),
            base_path: test_context.get_cache_dir().join("schemas"),
            root_schema: HashMap::new(),
        }
    }

    #[must_use]
    pub fn entry(mut self, name: impl Into<String>) -> Self {
        let name = name.into();
        let list_endpoint = format!("{}{}/", &self.api_path, &name);
        let schema = format!("{}schema/", &list_endpoint);
        let entry = RootSchemaEntry {
            list_endpoint,
            schema,
        };
        self.root_schema.insert(name, entry);
        self
    }

    pub fn write(&self, rel_path: impl AsRef<Path>) {
        let path = self.base_path.join(rel_path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let contents = serde_json::to_string(&self.root_schema).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    #[must_use]
    pub fn to_value(&self) -> Value {
        serde_json::to_value(&self.root_schema).unwrap()
    }

    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.root_schema).unwrap()
    }
}

#[allow(clippy::struct_excessive_bools)]
#[skip_serializing_none]
#[derive(Clone, Serialize)]
struct Field {
    blank: bool,
    default: Value,
    help_text: String,
    nullable: bool,
    readonly: bool,
    related_type: Option<String>,
    #[serde(rename = "type")]
    #[allow(clippy::struct_field_names)]
    field_type: String,
    unique: bool,
    valid_choices: Option<Vec<Value>>,
}

pub struct FieldBuilder {
    field: Field,
}

impl FieldBuilder {
    fn new() -> Self {
        Self {
            field: Field {
                blank: false,
                default: Value::Null,
                help_text: String::new(),
                nullable: true,
                readonly: false,
                related_type: None,
                field_type: String::from("string"),
                unique: false,
                valid_choices: None,
            },
        }
    }

    #[must_use]
    pub fn blank(mut self, value: bool) -> Self {
        self.field.blank = value;
        self
    }

    #[must_use]
    pub fn default(mut self, value: Value) -> Self {
        self.field.default = value;
        self
    }

    #[must_use]
    pub fn help_text(mut self, value: impl Into<String>) -> Self {
        self.field.help_text = value.into();
        self
    }

    #[must_use]
    pub fn nullable(mut self, value: bool) -> Self {
        self.field.nullable = value;
        self
    }

    #[must_use]
    pub fn readonly(mut self, value: bool) -> Self {
        self.field.readonly = value;
        self
    }

    #[must_use]
    pub fn related_type(mut self, value: impl Into<String>) -> Self {
        self.field.related_type = Some(value.into());
        self
    }

    #[must_use]
    pub fn field_type(mut self, value: impl Into<String>) -> Self {
        self.field.field_type = value.into();
        self
    }

    #[must_use]
    pub fn unique(mut self, value: bool) -> Self {
        self.field.unique = value;
        self
    }

    #[must_use]
    pub fn valid_choices(mut self, value: Option<Vec<Value>>) -> Self {
        self.field.valid_choices = value;
        self
    }
}

#[skip_serializing_none]
#[derive(Clone, Serialize)]
struct Schema {
    allowed_detail_http_methods: Vec<String>,
    allowed_list_http_methods: Vec<String>,
    default_format: String,
    default_limit: isize,
    fields: HashMap<String, Field>,
    filtering: HashMap<String, isize>,
    ordering: Vec<String>,
}

#[derive(Clone)]
pub struct SchemaBuilder {
    base_path: PathBuf,
    schema: Schema,
}

impl SchemaBuilder {
    pub(crate) fn new(test_context: &TestContext) -> Self {
        Self {
            base_path: test_context.get_cache_dir().join("schemas"),
            schema: Schema {
                allowed_detail_http_methods: ["get", "post", "put", "delete", "patch"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                allowed_list_http_methods: ["get", "post", "put", "delete", "patch"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                default_format: String::from("application/json"),
                default_limit: 20,
                fields: HashMap::new(),
                filtering: HashMap::new(),
                ordering: vec![],
            },
        }
    }

    #[must_use]
    pub fn field(
        mut self,
        name: impl Into<String>,
        f: impl FnOnce(FieldBuilder) -> FieldBuilder,
    ) -> Self {
        let name = name.into();
        self.schema.ordering.push(name.clone());
        self.schema
            .fields
            .insert(name, f(FieldBuilder::new()).field);
        self
    }

    pub fn write(&self, rel_path: impl AsRef<Path>) {
        let path = self.base_path.join(rel_path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let contents = serde_json::to_string(&self.schema).unwrap();
        std::fs::write(&path, contents).unwrap();
    }

    #[must_use]
    pub fn to_value(&self) -> Value {
        serde_json::to_value(&self.schema).unwrap()
    }

    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.schema).unwrap()
    }
}
