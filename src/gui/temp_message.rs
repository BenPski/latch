use std::collections::HashMap;

use iced::{
    widget::{container, text},
    Element,
};

use crate::{manager_message::ManagerMessage, message::Message, store::StoreChoice};

use super::gui_message::GUIMessage;

// first field is vault name
#[derive(Debug, Clone, Default)]
pub enum TempMessage {
    #[default]
    Empty,
    Delete(String, String),
    Get(String, String),
    New(String, String, StoreChoice, HashMap<String, String>),
    Update(String, String, StoreChoice, HashMap<String, String>),
}

impl TempMessage {
    pub fn needs_password(&self) -> bool {
        match self {
            Self::Empty => false,
            Self::Delete(_, _) => true,
            Self::Get(_, _) => true,
            Self::New(_, _, _, _) => true,
            Self::Update(_, _, _, _) => true,
        }
    }

    pub fn complete(&self) -> bool {
        match self {
            Self::Empty => true,
            Self::New(_, name, _, fields) => {
                let mut filled = true;
                for (_, value) in fields.iter() {
                    if value.is_empty() {
                        filled = false;
                        break;
                    }
                }
                !name.is_empty() && filled
            }
            Self::Update(_, name, _, fields) => {
                let mut filled = true;
                for (_, value) in fields.iter() {
                    if value.is_empty() {
                        filled = false;
                        break;
                    }
                }
                !name.is_empty() && filled
            }
            Self::Get(_, name) => !name.is_empty(),
            Self::Delete(_, name) => !name.is_empty(),
        }
    }

    pub fn with_password(&self, password: String) -> ManagerMessage {
        match self {
            Self::Delete(vault, key) => ManagerMessage::VaultMessage(
                vault.into(),
                Message::Delete(password, key.to_string()),
            ),
            Self::Get(vault, key) => {
                ManagerMessage::VaultMessage(vault.into(), Message::Get(password, key.to_string()))
            }
            Self::New(vault, key, choice, value) => ManagerMessage::VaultMessage(
                vault.into(),
                Message::Update(password, key.clone(), choice.convert(value).unwrap()),
            ),
            Self::Update(vault, key, choice, value) => ManagerMessage::VaultMessage(
                vault.into(),
                Message::Update(password, key.clone(), choice.convert(value).unwrap()),
            ),
            Self::Empty => ManagerMessage::Info,
        }
    }

    pub fn view(&self) -> Element<GUIMessage> {
        match self {
            TempMessage::Delete(vault, key) => {
                let info = text(format!("Working on deleting {} in {}", key, vault));
                container(info).into()
            }
            TempMessage::Get(vault, key) => {
                let info = text(format!("Working on getting {} in {}", key, vault));
                container(info).into()
            }
            TempMessage::New(vault, key, _, _) => {
                let info = text(format!("Working on a new entry {} in {}", key, vault));
                container(info).into()
            }
            TempMessage::Update(vault, key, _, _) => {
                let info = text(format!("Working on updating entry {} in {}", key, vault));
                container(info).into()
            }
            Self::Empty => {
                let info = text("Working on nothing");
                container(info).into()
            }
        }
    }
}
