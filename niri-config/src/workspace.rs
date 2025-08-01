use knuffel::errors::DecodeError;
use niri_ipc::WorkspaceReferenceArg;

#[derive(knuffel::Decode, Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    #[knuffel(argument)]
    pub name: WorkspaceName,
    #[knuffel(child, unwrap(argument))]
    pub open_on_output: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceName(pub String);

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum WorkspaceReference {
    Id(u64),
    Index(u8),
    Name(String),
}

impl From<WorkspaceReferenceArg> for WorkspaceReference {
    fn from(reference: WorkspaceReferenceArg) -> WorkspaceReference {
        match reference {
            WorkspaceReferenceArg::Id(id) => Self::Id(id),
            WorkspaceReferenceArg::Index(i) => Self::Index(i),
            WorkspaceReferenceArg::Name(n) => Self::Name(n),
        }
    }
}

impl<S: knuffel::traits::ErrorSpan> knuffel::DecodeScalar<S> for WorkspaceReference {
    fn type_check(
        type_name: &Option<knuffel::span::Spanned<knuffel::ast::TypeName, S>>,
        ctx: &mut knuffel::decode::Context<S>,
    ) {
        if let Some(type_name) = &type_name {
            ctx.emit_error(DecodeError::unexpected(
                type_name,
                "type name",
                "no type name expected for this node",
            ));
        }
    }

    fn raw_decode(
        val: &knuffel::span::Spanned<knuffel::ast::Literal, S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<WorkspaceReference, DecodeError<S>> {
        match &**val {
            knuffel::ast::Literal::String(ref s) => Ok(WorkspaceReference::Name(s.clone().into())),
            knuffel::ast::Literal::Int(ref value) => match value.try_into() {
                Ok(v) => Ok(WorkspaceReference::Index(v)),
                Err(e) => {
                    ctx.emit_error(DecodeError::conversion(val, e));
                    Ok(WorkspaceReference::Index(0))
                }
            },
            _ => {
                ctx.emit_error(DecodeError::unsupported(
                    val,
                    "Unsupported value, only numbers and strings are recognized",
                ));
                Ok(WorkspaceReference::Index(0))
            }
        }
    }
}

impl<S: knuffel::traits::ErrorSpan> knuffel::DecodeScalar<S> for WorkspaceName {
    fn type_check(
        type_name: &Option<knuffel::span::Spanned<knuffel::ast::TypeName, S>>,
        ctx: &mut knuffel::decode::Context<S>,
    ) {
        if let Some(type_name) = &type_name {
            ctx.emit_error(DecodeError::unexpected(
                type_name,
                "type name",
                "no type name expected for this node",
            ));
        }
    }

    fn raw_decode(
        val: &knuffel::span::Spanned<knuffel::ast::Literal, S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<WorkspaceName, DecodeError<S>> {
        #[derive(Debug)]
        struct WorkspaceNameSet(Vec<String>);
        match &**val {
            knuffel::ast::Literal::String(ref s) => {
                let mut name_set: Vec<String> = match ctx.get::<WorkspaceNameSet>() {
                    Some(h) => h.0.clone(),
                    None => Vec::new(),
                };

                if name_set.iter().any(|name| name.eq_ignore_ascii_case(s)) {
                    ctx.emit_error(DecodeError::unexpected(
                        val,
                        "named workspace",
                        format!("duplicate named workspace: {s}"),
                    ));
                    return Ok(Self(String::new()));
                }

                name_set.push(s.to_string());
                ctx.set(WorkspaceNameSet(name_set));
                Ok(Self(s.clone().into()))
            }
            _ => {
                ctx.emit_error(DecodeError::unsupported(
                    val,
                    "workspace names must be strings",
                ));
                Ok(Self(String::new()))
            }
        }
    }
}
