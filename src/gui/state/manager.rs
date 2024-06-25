use std::{
    collections::{BTreeMap},
    str::FromStr,
};

use crate::{
    config::{
        client_config::ClientConfig,
        internal_config::{BaseConfig, InternalConfig},
    },
    gui::{
        connection,
        entry::{EntryMessage},
        gui_message::GUIMessage,
        state::{entry::EntryState, new_entry::NewEntryState, password::PasswordState},
        temp_message::TempMessage,
        vault::{Vault, VaultMessage},
    },
    info::Info,
    manager_message::ManagerMessage,
    output::Output,
    reads::Reads,
    store::{Store, StoreChoice},
    utils, Password,
};
use iced::{
    alignment,
    widget::{button, column, container, row, scrollable, text},
    Application, Border, Color, Command, Element, Length, Subscription, Theme,
};

use iced_aw::menu::{self, Item, Menu, StyleSheet};
use iced_aw::style::MenuBarStyle;
use iced_aw::{menu_bar, menu_items, modal};
use iced_futures::MaybeSend;
use pants_gen::password::PasswordSpec;
use secrecy::{ExposeSecret, Secret};

use super::prompt::PromptState;

pub struct ManagerState {
    config: ClientConfig,
    info: Info,
    vaults: BTreeMap<String, Vault>,
    internal_state: Vec<InternalState>,
    temp_message: TempMessage,
    stored_clipboard: Option<Password>,
    state: ConnectionState,
}

impl Default for ManagerState {
    fn default() -> Self {
        let config: ClientConfig = <ClientConfig as BaseConfig>::load_err();
        Self {
            config,
            info: Info::default(),
            vaults: BTreeMap::new(),
            internal_state: Vec::new(),
            temp_message: TempMessage::default(),
            stored_clipboard: None,
            state: ConnectionState::Disconnected,
        }
    }
}

enum ConnectionState {
    Disconnected,
    Connected(connection::Connection),
}

impl ManagerState {
    fn get_password(&self) -> Option<Password> {
        for state in &self.internal_state {
            if let InternalState::Password(p) = state {
                return Some(p.password.clone());
            }
        }
        None
    }
    fn handle_password_submit(&mut self, password: Password) {
        let messages = match &self.temp_message {
            TempMessage::Get(vault, key) => {
                let message = self.temp_message.with_password(password);
                self.internal_state.push(
                    EntryState::from_entry(
                        vault.to_string(),
                        key.to_string(),
                        self.info.get(vault).unwrap().get(key).unwrap().to_string(),
                    )
                    .into(),
                );
                self.temp_message = TempMessage::Update(
                    vault.into(),
                    key.to_string(),
                    StoreChoice::default(),
                    StoreChoice::default().convert_default().as_hash(),
                );
                vec![message]
            }
            TempMessage::Delete(..) => {
                let message = self.temp_message.with_password(password);
                self.internal_state = vec![];
                self.temp_message = TempMessage::default();
                vec![message, ManagerMessage::Info]
            }
            TempMessage::New(..) => {
                let message = self.temp_message.with_password(password);
                self.internal_state = vec![];
                self.temp_message = TempMessage::default();
                vec![message, ManagerMessage::Info]
            }
            TempMessage::Update(..) => {
                let message = self.temp_message.with_password(password);
                self.internal_state = vec![];
                self.temp_message = TempMessage::default();
                vec![message, ManagerMessage::Info]
            }
            TempMessage::Empty => {
                self.internal_state = vec![];
                self.temp_message = TempMessage::default();
                vec![]
            }
            TempMessage::DeleteVault(..) => {
                let message = self.temp_message.with_password(password);
                self.internal_state = vec![];
                self.temp_message = TempMessage::default();
                vec![message, ManagerMessage::Info]
            }
            // non-sense
            TempMessage::DeleteEmptyVault(..) => {
                let message = self.temp_message.with_password(password);
                self.internal_state = vec![];
                self.temp_message = TempMessage::default();
                vec![message, ManagerMessage::Info]
            }
        };
        self.send_message(messages);
    }
    fn active_state(&self) -> Option<&InternalState> {
        self.internal_state.last()
    }
    fn active_state_mut(&mut self) -> Option<&mut InternalState> {
        self.internal_state.last_mut()
    }

    fn needs_password(&self) -> bool {
        self.temp_message.needs_password()
    }
    fn update(&mut self, info: Info) {
        let mut vaults = BTreeMap::new();
        for (name, schema) in info.data.iter() {
            let mut vault = Vault::new(name.into(), BTreeMap::new());
            vault.update(schema);
            if let Some(curr_vault) = self.vaults.get(name) {
                vault.expanded = curr_vault.expanded;
            }
            vaults.insert(name.into(), vault);
        }

        self.vaults = vaults;
        self.info = info;
    }
    fn update_entry(&mut self, data: Reads<Store>) {
        // TODO: check if robust, could be that a response was given to a lower down state, but I
        // find it unlikely it will get to be that way
        if let Some(InternalState::Entry(entry)) = self.active_state_mut() {
            for (key, value) in &data.data {
                if *key == entry.key {
                    entry.update(value.clone());
                }
            }
        }
        // seems dumb to loop twice
        if let TempMessage::Update(_, update_key, ref mut choice, ref mut update_value) =
            &mut self.temp_message
        {
            for (key, value) in data.data {
                if *update_key == key {
                    let (new_choice, new_values) = value.split();
                    *choice = new_choice;
                    *update_value = new_values;
                }
            }
        }
    }
    fn send_message(&mut self, messages: Vec<ManagerMessage>) {
        for message in messages {
            match self.state {
                ConnectionState::Disconnected => {}
                ConnectionState::Connected(ref mut connection) => {
                    connection.send(message);
                }
            }
        }
    }
    fn get_theme(&self) -> Theme {
        let map = utils::theme_map();
        map.get(&self.config.theme).cloned().unwrap_or_default()
    }
    fn view(&self) -> Element<GUIMessage> {
        let top_layer = self.internal_state.last().map(|state| state.view());

        let menu = |items| Menu::new(items).max_width(180.0).offset(0.0).spacing(5.0);
        let themes: Vec<Item<GUIMessage, iced::Theme, iced::Renderer>> = Theme::ALL
            .iter()
            .map(|t| {
                let item = if t.to_string() == self.config.theme {
                    action_selected_item(&t.to_string(), GUIMessage::ChangeTheme(t.clone()))
                } else {
                    action_item(&t.to_string(), GUIMessage::ChangeTheme(t.clone()))
                };
                Item::new(item)
            })
            .collect::<Vec<_>>();
        let theme_menu = Menu::new(themes).max_width(180.0).offset(15.0).spacing(5.0);
        #[rustfmt::skip]
        let menu = menu_bar!(
            (section_header("Config"), menu(menu_items!(
                (submenu_item("theme"), theme_menu)))
            )
        )
        .draw_path(menu::DrawPath::Backdrop)
        .style(|theme:&iced::Theme| menu::Appearance{
            path_border: Border{
                radius: [6.0; 4].into(),
                ..Default::default()
            },
            ..theme.appearance(&MenuBarStyle::Default)
        });

        let new_vault = button("New Vault").on_press(GUIMessage::NewVault);
        let content = scrollable(
            column(self.vaults.values().map(|v| {
                container(
                    v.view()
                        .map(move |message| GUIMessage::VaultMessage(message, v.name.clone())),
                )
                .padding(10)
                .into()
            }))
            .padding(10),
        );

        let info = self.temp_message.view();
        let primary = container(column![menu, new_vault, content, info]);
        modal(primary, top_layer)
            .backdrop(GUIMessage::Exit)
            .on_esc(GUIMessage::Exit)
            .align_y(alignment::Vertical::Center)
            .into()
    }
}

#[derive(Debug)]
enum InternalState {
    Password(PasswordState),
    Entry(EntryState),
    New(NewEntryState),
    Prompt(PromptState),
    // NewVault(NewVaultState),
}

impl From<PasswordState> for InternalState {
    fn from(value: PasswordState) -> Self {
        InternalState::Password(value)
    }
}

impl From<EntryState> for InternalState {
    fn from(value: EntryState) -> Self {
        InternalState::Entry(value)
    }
}

impl From<NewEntryState> for InternalState {
    fn from(value: NewEntryState) -> Self {
        InternalState::New(value)
    }
}

impl From<PromptState> for InternalState {
    fn from(value: PromptState) -> Self {
        InternalState::Prompt(value)
    }
}

// impl From<NewVaultState> for InternalState {
//     fn from(value: NewVaultState) -> Self {
//         InternalState::NewVault(value)
//     }
// }

fn delayed_command(
    time: u64,
    callback: impl FnOnce(()) -> GUIMessage + 'static + MaybeSend,
) -> Command<GUIMessage> {
    Command::perform(
        async move {
            let _ = async_std::task::sleep(std::time::Duration::from_secs(time)).await;
        },
        callback,
    )
}

impl InternalState {
    fn view(&self) -> Element<GUIMessage> {
        match self {
            Self::Password(password_state) => password_state.view(),
            Self::New(new_state) => new_state.view(),
            Self::Entry(entry_state) => entry_state.view(),
            Self::Prompt(prompt_state) => prompt_state.view(),
            // Self::NewVault(new_vault_state) => new_vault_state.view(),
        }
    }
}

impl Application for ManagerState {
    type Flags = ();
    type Theme = Theme;
    type Message = GUIMessage;
    type Executor = iced::executor::Default;

    fn title(&self) -> String {
        "Pants".to_string()
    }

    fn new(_flags: Self::Flags) -> (Self, iced::Command<Self::Message>) {
        (Self::default(), Command::none())
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            GUIMessage::Event(event) => match event {
                connection::Event::Connected(connection) => {
                    self.state = ConnectionState::Connected(connection);
                    self.send_message(vec![ManagerMessage::Info]);
                }
                connection::Event::Disconnected => {
                    self.state = ConnectionState::Disconnected;
                }
                connection::Event::ReceiveOutput(output) => match output {
                    Output::Info(info) => {
                        println!("Received info: {:?}", info);
                        self.update(info);
                    }
                    Output::Read(value) => {
                        println!("Received read: {:?}", value);
                        self.update_entry(value);
                    }
                    Output::Nothing => {}
                    _ => todo!(),
                },

                connection::Event::ReceiveError => {
                    println!("Encountered error");
                    self.internal_state = vec![];
                    self.temp_message = TempMessage::default();
                }
            },
            // GUIMessage::Send(message) => self.send_message(vec![message]),
            GUIMessage::VaultMessage(message, vault) => match message {
                VaultMessage::Entry(entry_message, key) => match entry_message {
                    EntryMessage::Delete => {
                        self.temp_message = TempMessage::Delete(vault, key);
                        if self.needs_password() {
                            self.internal_state.push(PasswordState::default().into());
                        }
                    }
                    EntryMessage::View => {
                        self.temp_message = TempMessage::Get(vault, key.clone());

                        if self.needs_password() {
                            self.internal_state.push(PasswordState::default().into());
                        }
                    }
                },
                VaultMessage::NewEntry => {
                    self.temp_message = TempMessage::New(
                        vault.to_string(),
                        String::new(),
                        StoreChoice::default(),
                        StoreChoice::default().convert_default().as_hash(),
                    );
                    self.internal_state
                        .push(NewEntryState::for_vault(vault).into());
                }
                VaultMessage::Delete => {
                    if self.info.get(&vault).unwrap().is_empty() {
                        self.send_message(vec![
                            ManagerMessage::DeleteEmptyVault(vault),
                            ManagerMessage::Info,
                        ]);
                        // self.temp_message = TempMessage::DeleteEmptyVault(vault);
                    } else {
                        self.temp_message = TempMessage::DeleteVault(vault);
                    }
                    if self.needs_password() {
                        self.internal_state.push(PasswordState::default().into());
                    }
                }
                VaultMessage::Toggle(name) => {
                    if let Some(value) = self.vaults.get_mut(&name) {
                        value.toggle();
                    }
                }
            },

            GUIMessage::PasswordChanged(p) => {
                if let Some(InternalState::Password(password_state)) = self.active_state_mut() {
                    password_state.password = p;
                }
            }
            GUIMessage::PasswordConfirmChanged(p) => {
                if let Some(InternalState::Password(password_state)) = self.active_state_mut() {
                    password_state.confirm = Some(p);
                }
            }
            GUIMessage::PromptChanged(p) => {
                if let Some(InternalState::Prompt(prompt_state)) = self.active_state_mut() {
                    prompt_state.vault = p;
                }
            }

            GUIMessage::ChangeName(n) => {
                if let Some(InternalState::New(new_state)) = self.active_state_mut() {
                    new_state.name.clone_from(&n);
                }
                if let TempMessage::New(_, ref mut key, _, _) = &mut self.temp_message {
                    *key = n;
                }
            }
            GUIMessage::SelectStyle(choice) => {
                if let Some(InternalState::New(new_state)) = self.active_state_mut() {
                    new_state.choice = choice;
                    new_state.value = choice.convert_default().as_hash();
                }
                if let TempMessage::New(_, _, ref mut style, ref mut value) = &mut self.temp_message
                {
                    *style = choice;
                    *value = choice.convert_default().as_hash();
                }
            }
            GUIMessage::UpdateField(k, v) => {
                match self.active_state_mut() {
                    Some(InternalState::New(new_state)) => {
                        new_state.value.insert(k.clone(), v.clone());
                    }
                    Some(InternalState::Entry(entry_state)) => {
                        entry_state.value.insert(k.clone(), v.clone());
                    }
                    _ => {}
                };
                match &mut self.temp_message {
                    TempMessage::New(_, _, _, ref mut value) => {
                        value.insert(k, v);
                    }
                    TempMessage::Update(_, _, _, ref mut value) => {
                        value.insert(k, v);
                    }
                    _ => {}
                };
            }
            GUIMessage::GeneratePassword => {
                let spec = PasswordSpec::from_str(&self.config.password_spec).unwrap();
                let password: Secret<String> = spec.generate().unwrap().into();
                match self.active_state_mut() {
                    Some(InternalState::New(new_state)) => {
                        new_state
                            .value
                            .insert("password".to_string(), password.clone());
                    }
                    Some(InternalState::Entry(entry_state)) => {
                        entry_state
                            .value
                            .insert("password".to_string(), password.clone());
                    }
                    _ => {}
                };
                match &mut self.temp_message {
                    TempMessage::New(_, _, _, ref mut value) => {
                        value.insert("password".to_string(), password);
                    }
                    TempMessage::Update(_, _, _, ref mut value) => {
                        value.insert("password".to_string(), password);
                    }
                    _ => {}
                };
            }

            GUIMessage::Submit => {
                if let Some(active_state) = self.active_state() {
                    match active_state {
                        InternalState::Password(password_state) => {
                            if password_state.valid() {
                                self.handle_password_submit(password_state.password.clone());
                            }
                        }
                        InternalState::New(new_state) => {
                            if let Some(schema) = self.info.get(&new_state.vault) {
                                println!("{:?}", schema);
                                if self.temp_message.complete() {
                                    if schema.is_empty() {
                                        self.internal_state.push(PasswordState::new(true).into());
                                    } else if !schema.data.contains_key(&new_state.name) {
                                        self.internal_state.push(PasswordState::default().into());
                                    }
                                }
                            }
                        }
                        InternalState::Entry(entry_state) => {
                            if let Some(schema) = self.info.get(&entry_state.vault) {
                                if schema.data.contains_key(&entry_state.key)
                                    && self.temp_message.complete()
                                {
                                    if let Some(password) = self.get_password() {
                                        let message = self.temp_message.with_password(password);
                                        self.send_message(vec![message]);
                                        self.temp_message = TempMessage::default();
                                        self.internal_state = vec![];
                                    } else {
                                        self.internal_state.push(PasswordState::default().into());
                                    }
                                }
                            }
                        }
                        // InternalState::NewVault(new_vault_state) => {
                        //     if !self.info.data.contains_key(&new_vault_state.vault)
                        //         && self.temp_message.complete()
                        //     {
                        //         let new_message =
                        //             ManagerMessage::NewVault(new_vault_state.vault.clone());
                        //         self.internal_state.push(PasswordState::default().into());
                        //         self.send_message(vec![new_message])
                        //     }
                        // }
                        InternalState::Prompt(prompt_state) => {
                            if !self.info.data.contains_key(&prompt_state.vault)
                                && !prompt_state.vault.is_empty()
                            {
                                let message = ManagerMessage::NewVault(prompt_state.vault.clone());
                                self.send_message(vec![message, ManagerMessage::Info]);
                                self.internal_state.pop();
                            }
                        }
                    }
                }
            }
            GUIMessage::Exit => {
                if let Some(active_state) = self.active_state() {
                    match active_state {
                        InternalState::Password(_password_state) => {
                            match self.temp_message {
                                TempMessage::Delete(..) => {
                                    self.temp_message = TempMessage::default();
                                }
                                TempMessage::Get(..) => {
                                    self.temp_message = TempMessage::default();
                                }
                                TempMessage::DeleteVault(..) => {
                                    self.temp_message = TempMessage::default();
                                }
                                TempMessage::DeleteEmptyVault(..) => {
                                    self.temp_message = TempMessage::default();
                                }
                                TempMessage::Update(..) => {}
                                TempMessage::New(..) => {}
                                TempMessage::Empty => {}
                            }
                            self.internal_state.pop();
                        }
                        InternalState::Entry(_entry_state) => {
                            self.temp_message = TempMessage::default();
                            self.internal_state = vec![];
                        }
                        InternalState::New(_new_state) => {
                            self.temp_message = TempMessage::default();
                            self.internal_state = vec![];
                        }
                        InternalState::Prompt(_) => {
                            self.internal_state.pop();
                        }
                    }
                }
            }

            GUIMessage::ShowPassword => {
                if let Some(InternalState::Entry(entry_state)) = self.active_state_mut() {
                    entry_state.hidden = false;
                }
            }
            GUIMessage::HidePassword => {
                if let Some(InternalState::Entry(entry_state)) = self.active_state_mut() {
                    entry_state.hidden = true;
                }
            }
            GUIMessage::CopyPassword => {
                if let Some(InternalState::Entry(entry_state)) = self.active_state_mut() {
                    if let Some(p) = entry_state.get_password() {
                        return Command::batch(vec![
                            iced::clipboard::read(|s| {
                                GUIMessage::CopyClipboard(s.map(|x| x.into()))
                            }),
                            iced::clipboard::write(p.expose_secret().into()),
                            delayed_command(self.config.clipboard_time, |_| {
                                GUIMessage::ClearClipboard
                            }),
                        ]);
                    }
                }
            }
            GUIMessage::CopyClipboard(data) => self.stored_clipboard = data,
            GUIMessage::ClearClipboard => {
                let contents: Secret<String> = self
                    .stored_clipboard
                    .clone()
                    .unwrap_or_else(|| Secret::new(String::new()));
                self.stored_clipboard = None;
                return iced::clipboard::write(contents.expose_secret().into());
            }
            GUIMessage::NewVault => self.internal_state.push(PromptState::default().into()),
            GUIMessage::ChangeTheme(theme) => {
                self.config.theme = theme.to_string();
                // ignoring save result for now
                let _ = self.config.save();
            }
        }

        Command::none()
    }

    fn view(&self) -> Element<Self::Message> {
        self.view()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        connection::connect().map(GUIMessage::Event)
    }

    fn theme(&self) -> Theme {
        self.get_theme()
    }
}
struct ButtonStyle;
impl button::StyleSheet for ButtonStyle {
    type Style = iced::Theme;

    fn active(&self, style: &Self::Style) -> button::Appearance {
        button::Appearance {
            text_color: style.extended_palette().background.base.text,
            background: Some(Color::TRANSPARENT.into()),
            // background: Some(Color::from([1.0; 3]).into()),
            border: Border {
                radius: [6.0; 4].into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn hovered(&self, style: &Self::Style) -> button::Appearance {
        let plt = style.extended_palette();

        button::Appearance {
            background: Some(plt.primary.weak.color.into()),
            text_color: plt.primary.weak.text,
            ..self.active(style)
        }
    }
}

fn section_header<'a>(label: &str) -> button::Button<'a, GUIMessage, iced::Theme, iced::Renderer> {
    base_button(text(label), None)
}

fn submenu_item<'a>(label: &str) -> button::Button<'a, GUIMessage, iced::Theme, iced::Renderer> {
    base_button(
        row![
            text(label).width(Length::Fill),
            text(">").width(Length::Shrink)
        ],
        None,
    )
    .width(Length::Fill)
}

// fn menu_item<'a>(label: &str) -> button::Button<'a, GUIMessage, iced::Theme, iced::Renderer> {
//     base_button(text(label), None).width(Length::Fill)
// }

fn action_item<'a>(
    label: &str,
    message: GUIMessage,
) -> button::Button<'a, GUIMessage, iced::Theme, iced::Renderer> {
    base_button(text(label), Some(message)).width(Length::Fill)
}

fn action_selected_item<'a>(
    label: &str,
    message: GUIMessage,
) -> button::Button<'a, GUIMessage, iced::Theme, iced::Renderer> {
    base_button(
        row![
            text(label).width(Length::Fill),
            text("<").width(Length::Shrink)
        ],
        Some(message),
    )
}

fn base_button<'a>(
    content: impl Into<Element<'a, GUIMessage, iced::Theme, iced::Renderer>>,
    msg: Option<GUIMessage>,
) -> button::Button<'a, GUIMessage, iced::Theme, iced::Renderer> {
    if let Some(msg) = msg {
        button(content)
            .padding([4, 8])
            .style(iced::theme::Button::Custom(Box::new(ButtonStyle {})))
            .on_press(msg)
    } else {
        button(content)
            .padding([4, 8])
            .style(iced::theme::Button::Custom(Box::new(ButtonStyle {})))
    }
}