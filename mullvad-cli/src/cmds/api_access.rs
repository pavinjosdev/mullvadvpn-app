use anyhow::{anyhow, Result};
use mullvad_management_interface::MullvadProxyClient;
use mullvad_types::access_method::{AccessMethod, AccessMethodSetting, CustomAccessMethod};
use std::net::IpAddr;

use clap::{Args, Subcommand};
use talpid_types::net::openvpn::SHADOWSOCKS_CIPHERS;

#[derive(Subcommand, Debug, Clone)]
pub enum ApiAccess {
    /// List the configured API access methods
    List,
    /// Add a custom API access method
    #[clap(subcommand)]
    Add(AddCustomCommands),
    /// Edit an API access method
    Edit(EditCustomCommands),
    /// Remove an API access method
    Remove(SelectItem),
    /// Enable an API access method
    Enable(SelectItem),
    /// Disable an API access method
    Disable(SelectItem),
    /// Test an API access method
    Test(SelectItem),
    /// Force the use of a specific API access method.
    ///
    /// Selecting "Mullvad Bridges" respects your current bridge settings.
    Use(SelectItem),
}

impl ApiAccess {
    pub async fn handle(self) -> Result<()> {
        match self {
            ApiAccess::List => {
                Self::list().await?;
            }
            ApiAccess::Add(cmd) => {
                Self::add(cmd).await?;
            }
            ApiAccess::Edit(cmd) => Self::edit(cmd).await?,
            ApiAccess::Remove(cmd) => Self::remove(cmd).await?,
            ApiAccess::Enable(cmd) => {
                Self::enable(cmd).await?;
            }
            ApiAccess::Disable(cmd) => {
                Self::disable(cmd).await?;
            }
            ApiAccess::Test(cmd) => {
                Self::test(cmd).await?;
            }
            ApiAccess::Use(cmd) => {
                Self::set(cmd).await?;
            }
        };
        Ok(())
    }

    /// Show all API access methods.
    async fn list() -> Result<()> {
        let mut rpc = MullvadProxyClient::new().await?;
        for (index, api_access_method) in rpc.get_api_access_methods().await?.iter().enumerate() {
            println!(
                "{}. {}",
                index + 1,
                pp::ApiAccessMethodFormatter::new(api_access_method)
            );
        }
        Ok(())
    }

    /// Add a custom API access method.
    async fn add(cmd: AddCustomCommands) -> Result<()> {
        let mut rpc = MullvadProxyClient::new().await?;
        let name = cmd.name();
        let enabled = cmd.enabled();
        let access_method = AccessMethod::try_from(cmd)?;
        rpc.add_access_method(name, enabled, access_method).await?;
        Ok(())
    }

    /// Remove an API access method.
    async fn remove(cmd: SelectItem) -> Result<()> {
        let mut rpc = MullvadProxyClient::new().await?;
        let access_method = Self::get_access_method(&mut rpc, &cmd).await?;
        rpc.remove_access_method(access_method.get_id())
            .await
            .map_err(Into::<anyhow::Error>::into)
    }

    /// Edit the data of an API access method.
    async fn edit(cmd: EditCustomCommands) -> Result<()> {
        let mut rpc = MullvadProxyClient::new().await?;
        let mut api_access_method = Self::get_access_method(&mut rpc, &cmd.item).await?;

        // Create a new access method combining the new params with the previous values
        let access_method = match api_access_method.as_custom() {
            None => return Err(anyhow!("Can not edit built-in access method")),
            Some(x) => match x.clone() {
                CustomAccessMethod::Shadowsocks(shadowsocks) => {
                    let ip = cmd.params.ip.unwrap_or(shadowsocks.peer.ip()).to_string();
                    let port = cmd.params.port.unwrap_or(shadowsocks.peer.port());
                    let password = cmd.params.password.unwrap_or(shadowsocks.password);
                    let cipher = cmd.params.cipher.unwrap_or(shadowsocks.cipher);
                    mullvad_types::access_method::Shadowsocks::from_args(ip, port, cipher, password)
                        .map(AccessMethod::from)
                }
                CustomAccessMethod::Socks5(socks) => match socks {
                    mullvad_types::access_method::Socks5::Local(local) => {
                        let ip = cmd.params.ip.unwrap_or(local.peer.ip()).to_string();
                        let port = cmd.params.port.unwrap_or(local.peer.port());
                        let local_port = cmd.params.local_port.unwrap_or(local.port);
                        mullvad_types::access_method::Socks5Local::from_args(ip, port, local_port)
                            .map(AccessMethod::from)
                    }
                    mullvad_types::access_method::Socks5::Remote(remote) => {
                        let ip = cmd.params.ip.unwrap_or(remote.peer.ip()).to_string();
                        let port = cmd.params.port.unwrap_or(remote.peer.port());
                        mullvad_types::access_method::Socks5Remote::from_args(ip, port)
                            .map(AccessMethod::from)
                    }
                },
            },
        };

        if let Some(name) = cmd.params.name {
            api_access_method.name = name;
        };
        if let Some(access_method) = access_method {
            api_access_method.access_method = access_method;
        }

        rpc.update_access_method(api_access_method).await?;

        Ok(())
    }

    /// Enable a custom API access method.
    async fn enable(item: SelectItem) -> Result<()> {
        let mut rpc = MullvadProxyClient::new().await?;
        let access_method = Self::get_access_method(&mut rpc, &item).await?;
        rpc.enable_access_method(access_method.get_id()).await?;
        Ok(())
    }

    /// Disable a custom API access method.
    async fn disable(item: SelectItem) -> Result<()> {
        let mut rpc = MullvadProxyClient::new().await?;
        let access_method = Self::get_access_method(&mut rpc, &item).await?;
        rpc.disable_access_method(access_method.get_id()).await?;
        Ok(())
    }

    /// Test an access method to see if it successfully reaches the Mullvad API.
    async fn test(item: SelectItem) -> Result<()> {
        let mut rpc = MullvadProxyClient::new().await?;
        let access_method = Self::get_access_method(&mut rpc, &item).await?;
        rpc.set_access_method(access_method.get_id()).await?;
        // Make the daemon perform an network request which involves talking to the Mullvad API.
        match rpc.get_api_addresses().await {
            Ok(_) => println!("Connected to the Mullvad API!"),
            Err(_) => println!(
                "Could *not* connect to the Mullvad API using access method \"{}\"",
                access_method.name
            ),
        }

        Ok(())
    }

    /// Force the use of a specific access method when trying to reach the
    /// Mullvad API. If this method fails, the daemon will resume the automatic
    /// roll-over behavior (which is the default).
    async fn set(item: SelectItem) -> Result<()> {
        let mut rpc = MullvadProxyClient::new().await?;
        let access_method = Self::get_access_method(&mut rpc, &item).await?;
        rpc.set_access_method(access_method.get_id()).await?;
        Ok(())
    }

    async fn get_access_method(
        rpc: &mut MullvadProxyClient,
        item: &SelectItem,
    ) -> Result<AccessMethodSetting> {
        rpc.get_api_access_methods()
            .await?
            .get(item.as_array_index()?)
            .cloned()
            .ok_or(anyhow!(format!("Access method {} does not exist", item)))
    }
}

#[derive(Subcommand, Debug, Clone)]
pub enum AddCustomCommands {
    /// Configure a local SOCKS5 proxy
    #[clap(subcommand)]
    Socks5(AddSocks5Commands),
    /// Configure Shadowsocks proxy
    Shadowsocks {
        /// An easy to remember name for this custom proxy
        name: String,
        /// The IP of the remote Shadowsocks server
        remote_ip: IpAddr,
        /// The port of the remote Shadowsocks server
        #[arg(default_value = "443")]
        remote_port: u16,
        /// Password for authentication
        #[arg(default_value = "mullvad")]
        password: String,
        /// Cipher to use
        #[arg(value_parser = SHADOWSOCKS_CIPHERS, default_value = "aes-256-gcm")]
        cipher: String,
        /// Disable the use of this custom access method. It has to be manually
        /// enabled at a later stage to be used when accessing the Mullvad API.
        #[arg(default_value_t = false, short, long)]
        disabled: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum AddSocks5Commands {
    /// Configure a remote SOCKS5 proxy
    Remote {
        /// An easy to remember name for this custom proxy
        name: String,
        /// The IP of the remote proxy server
        remote_ip: IpAddr,
        /// The port of the remote proxy server
        remote_port: u16,
        /// Disable the use of this custom access method. It has to be manually
        /// enabled at a later stage to be used when accessing the Mullvad API.
        #[arg(default_value_t = false, short, long)]
        disabled: bool,
    },
    /// Configure a local SOCKS5 proxy
    Local {
        /// An easy to remember name for this custom proxy
        name: String,
        /// The port that the server on localhost is listening on
        local_port: u16,
        /// The IP of the remote peer
        remote_ip: IpAddr,
        /// The port of the remote peer
        remote_port: u16,
        /// Disable the use of this custom access method. It has to be manually
        /// enabled at a later stage to be used when accessing the Mullvad API.
        #[arg(default_value_t = false, short, long)]
        disabled: bool,
    },
}

impl AddCustomCommands {
    fn name(&self) -> String {
        match self {
            AddCustomCommands::Shadowsocks { name, .. } => name,
            AddCustomCommands::Socks5(socks) => match socks {
                AddSocks5Commands::Remote { name, .. } => name,
                AddSocks5Commands::Local { name, .. } => name,
            },
        }
        .clone()
    }

    fn enabled(&self) -> bool {
        match self {
            AddCustomCommands::Shadowsocks { disabled, .. } => !disabled,
            AddCustomCommands::Socks5(socks) => match socks {
                AddSocks5Commands::Remote { disabled, .. } => !disabled,
                AddSocks5Commands::Local { disabled, .. } => !disabled,
            },
        }
    }
}

/// A minimal wrapper type allowing the user to supply a list index to some
/// Access Method.
#[derive(Args, Debug, Clone)]
pub struct SelectItem {
    /// Which access method to pick
    index: usize,
}

impl SelectItem {
    /// Transform human-readable (1-based) index to 0-based indexing.
    pub fn as_array_index(&self) -> Result<usize> {
        self.index
            .checked_sub(1)
            .ok_or(anyhow!("Access method 0 does not exist"))
    }
}

impl std::fmt::Display for SelectItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.index)
    }
}

#[derive(Args, Debug, Clone)]
pub struct EditCustomCommands {
    /// Which API access method to edit
    #[clap(flatten)]
    item: SelectItem,
    /// Editing parameters
    #[clap(flatten)]
    params: EditParams,
}

#[derive(Args, Debug, Clone)]
pub struct EditParams {
    /// Name of the API access method in the Mullvad client [All]
    #[arg(long)]
    name: Option<String>,
    /// Password for authentication [Shadowsocks]
    #[arg(long)]
    password: Option<String>,
    /// Cipher to use [Shadowsocks]
    #[arg(value_parser = SHADOWSOCKS_CIPHERS, long)]
    cipher: Option<String>,
    /// The IP of the remote proxy server [Socks5 (Local & Remote proxy), Shadowsocks]
    #[arg(long)]
    ip: Option<IpAddr>,
    /// The port of the remote proxy server [Socks5 (Local & Remote proxy), Shadowsocks]
    #[arg(long)]
    port: Option<u16>,
    /// The port that the server on localhost is listening on [Socks5 (Local proxy)]
    #[arg(long)]
    local_port: Option<u16>,
}

/// Implement conversions from CLI types to Daemon types.
///
/// Since these are not supposed to be used outside of the CLI,
/// we define them in a hidden-away module.
mod conversions {
    use anyhow::{anyhow, Error};
    use mullvad_types::access_method as daemon_types;

    use super::{AddCustomCommands, AddSocks5Commands};

    impl TryFrom<AddCustomCommands> for daemon_types::AccessMethod {
        type Error = Error;

        fn try_from(value: AddCustomCommands) -> Result<Self, Self::Error> {
            Ok(match value {
                AddCustomCommands::Socks5(socks) => match socks {
                    AddSocks5Commands::Local {
                        local_port,
                        remote_ip,
                        remote_port,
                        name: _,
                        disabled: _,
                    } => {
                        println!("Adding Local SOCKS5-proxy: localhost:{local_port} => {remote_ip}:{remote_port}");
                        let socks_proxy = daemon_types::Socks5::Local(
                            daemon_types::Socks5Local::from_args(
                                remote_ip.to_string(),
                                remote_port,
                                local_port,
                            )
                            .ok_or(anyhow!("Could not create a local Socks5 api proxy"))?,
                        );
                        daemon_types::AccessMethod::from(socks_proxy)
                    }
                    AddSocks5Commands::Remote {
                        remote_ip,
                        remote_port,
                        name: _,
                        disabled: _,
                    } => {
                        println!("Adding SOCKS5-proxy: {remote_ip}:{remote_port}");
                        let socks_proxy = daemon_types::Socks5::Remote(
                            daemon_types::Socks5Remote::from_args(
                                remote_ip.to_string(),
                                remote_port,
                            )
                            .ok_or(anyhow!("Could not create a remote Socks5 api proxy"))?,
                        );
                        daemon_types::AccessMethod::from(socks_proxy)
                    }
                },
                AddCustomCommands::Shadowsocks {
                    remote_ip,
                    remote_port,
                    password,
                    cipher,
                    name: _,
                    disabled: _,
                } => {
                    println!(
                "Adding Shadowsocks-proxy: {password} @ {remote_ip}:{remote_port} using {cipher}"
                    );
                    let shadowsocks_proxy = daemon_types::Shadowsocks::from_args(
                        remote_ip.to_string(),
                        remote_port,
                        cipher,
                        password,
                    )
                    .ok_or(anyhow!("Could not create a Shadowsocks api proxy"))?;
                    daemon_types::AccessMethod::from(shadowsocks_proxy)
                }
            })
        }
    }
}

/// Pretty printing of [`ApiAccessMethod`]s
mod pp {
    use mullvad_types::access_method::{
        AccessMethod, AccessMethodSetting, CustomAccessMethod, Socks5,
    };

    pub struct ApiAccessMethodFormatter<'a> {
        api_access_method: &'a AccessMethodSetting,
    }

    impl<'a> ApiAccessMethodFormatter<'a> {
        pub fn new(api_access_method: &'a AccessMethodSetting) -> ApiAccessMethodFormatter<'a> {
            ApiAccessMethodFormatter { api_access_method }
        }
    }

    impl<'a> std::fmt::Display for ApiAccessMethodFormatter<'a> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            use crate::print_option;

            let write_status = |f: &mut std::fmt::Formatter<'_>, enabled: bool| {
                if enabled {
                    write!(f, " *")
                } else {
                    write!(f, "")
                }
            };

            match &self.api_access_method.access_method {
                AccessMethod::BuiltIn(method) => {
                    write!(f, "{}", method.canonical_name())?;
                    write_status(f, self.api_access_method.enabled())
                }
                AccessMethod::Custom(method) => match &method {
                    CustomAccessMethod::Shadowsocks(shadowsocks) => {
                        write!(f, "{}", self.api_access_method.get_name())?;
                        write_status(f, self.api_access_method.enabled())?;
                        writeln!(f)?;
                        print_option!("Protocol", format!("Shadowsocks [{}]", shadowsocks.cipher));
                        print_option!("Peer", shadowsocks.peer);
                        print_option!("Password", shadowsocks.password);
                        Ok(())
                    }
                    CustomAccessMethod::Socks5(socks) => match socks {
                        Socks5::Remote(remote) => {
                            write!(f, "{}", self.api_access_method.get_name())?;
                            write_status(f, self.api_access_method.enabled())?;
                            writeln!(f)?;
                            print_option!("Protocol", "Socks5");
                            print_option!("Peer", remote.peer);
                            Ok(())
                        }
                        Socks5::Local(local) => {
                            write!(f, "{}", self.api_access_method.get_name())?;
                            write_status(f, self.api_access_method.enabled())?;
                            writeln!(f)?;
                            print_option!("Protocol", "Socks5 (local)");
                            print_option!("Peer", local.peer);
                            print_option!("Local port", local.port);
                            Ok(())
                        }
                    },
                },
            }
        }
    }
}
