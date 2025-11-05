use i2ptunnel::I2PProxyDaemon;

fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("i2ptunnel=info".parse().unwrap()),
        )
        .init();

    tracing::info!("I2P Tunnel started");
    tracing::info!("This daemon is meant to be used as a Python library");
    tracing::info!("Import it in Python: from i2ptunnel import I2PProxyDaemon");
}

