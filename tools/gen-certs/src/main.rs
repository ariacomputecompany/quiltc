use clap::Parser;
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    KeyPair, KeyUsagePurpose, SanType,
};
use std::fs;
use std::path::PathBuf;
use time::{Duration, OffsetDateTime};

#[derive(Parser, Debug)]
#[command(name = "quilt-gen-certs")]
#[command(about = "Generate TLS certificates for Quilt Mesh")]
struct Args {
    /// Output directory for certificates
    #[arg(long, default_value = "certs")]
    output: PathBuf,

    /// Control plane hostname(s) for SAN
    #[arg(long, default_value = "localhost")]
    control_host: Vec<String>,

    /// Runtime hostname(s) for SAN
    #[arg(long, default_value = "localhost")]
    runtime_host: Vec<String>,

    /// Certificate validity in days
    #[arg(long, default_value = "365")]
    days: u32,
}

fn main() {
    let args = Args::parse();

    // Create output directory
    fs::create_dir_all(&args.output).expect("Failed to create output directory");

    let validity = Duration::days(args.days as i64);

    // Generate CA
    println!("Generating CA certificate...");
    let (ca_cert, ca_key_pair) = generate_ca(validity);
    let ca_cert_pem = ca_cert.pem();
    let ca_key_pem = ca_key_pair.serialize_pem();

    write_file(&args.output.join("ca.pem"), &ca_cert_pem);
    write_file(&args.output.join("ca.key"), &ca_key_pem);
    println!("  -> ca.pem, ca.key");

    // Generate control plane server cert
    println!("Generating control plane server certificate...");
    let (cert_pem, key_pem) = generate_server_cert(
        "quilt-control",
        &args.control_host,
        validity,
        &ca_cert,
        &ca_key_pair,
    );
    write_file(&args.output.join("control-server.pem"), &cert_pem);
    write_file(&args.output.join("control-server.key"), &key_pem);
    println!("  -> control-server.pem, control-server.key");

    // Generate runtime server cert
    println!("Generating runtime server certificate...");
    let (cert_pem, key_pem) = generate_server_cert(
        "quilt-runtime",
        &args.runtime_host,
        validity,
        &ca_cert,
        &ca_key_pair,
    );
    write_file(&args.output.join("runtime-server.pem"), &cert_pem);
    write_file(&args.output.join("runtime-server.key"), &key_pem);
    println!("  -> runtime-server.pem, runtime-server.key");

    // Generate agent client cert
    println!("Generating agent client certificate...");
    let (cert_pem, key_pem) = generate_client_cert("quilt-agent", validity, &ca_cert, &ca_key_pair);
    write_file(&args.output.join("agent-client.pem"), &cert_pem);
    write_file(&args.output.join("agent-client.key"), &key_pem);
    println!("  -> agent-client.pem, agent-client.key");

    println!("\nAll certificates generated in {:?}", args.output);
    println!("\nUsage:");
    println!("  Control plane: --tls-cert {0}/control-server.pem --tls-key {0}/control-server.key --tls-ca {0}/ca.pem", args.output.display());
    println!("  Runtime:       --tls-cert {0}/runtime-server.pem --tls-key {0}/runtime-server.key --tls-ca {0}/ca.pem", args.output.display());
    println!("  Agent:         --tls-ca {0}/ca.pem --tls-cert {0}/agent-client.pem --tls-key {0}/agent-client.key", args.output.display());
}

fn generate_ca(validity: Duration) -> (rcgen::Certificate, KeyPair) {
    let key_pair = KeyPair::generate().expect("Failed to generate CA key pair");

    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "Quilt Mesh CA");
    dn.push(DnType::OrganizationName, "Quilt Mesh");
    params.distinguished_name = dn;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    params.not_before = OffsetDateTime::now_utc();
    params.not_after = OffsetDateTime::now_utc() + validity;

    let cert = params
        .self_signed(&key_pair)
        .expect("Failed to generate CA certificate");

    (cert, key_pair)
}

fn generate_server_cert(
    cn: &str,
    hosts: &[String],
    validity: Duration,
    ca_cert: &rcgen::Certificate,
    ca_key: &KeyPair,
) -> (String, String) {
    let key_pair = KeyPair::generate().expect("Failed to generate server key pair");

    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, cn);
    dn.push(DnType::OrganizationName, "Quilt Mesh");
    params.distinguished_name = dn;
    params.is_ca = IsCa::NoCa;
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params.not_before = OffsetDateTime::now_utc();
    params.not_after = OffsetDateTime::now_utc() + validity;

    // Add SANs
    let mut sans = Vec::new();
    for host in hosts {
        if let Ok(ip) = host.parse::<std::net::IpAddr>() {
            sans.push(SanType::IpAddress(ip));
        } else {
            sans.push(SanType::DnsName(
                host.clone().try_into().expect("Invalid DNS name"),
            ));
        }
    }
    // Always include localhost and 127.0.0.1
    sans.push(SanType::DnsName(
        "localhost".to_string().try_into().unwrap(),
    ));
    sans.push(SanType::IpAddress(std::net::IpAddr::V4(
        std::net::Ipv4Addr::LOCALHOST,
    )));
    params.subject_alt_names = sans;

    let cert = params
        .signed_by(&key_pair, ca_cert, ca_key)
        .expect("Failed to sign server certificate");

    (cert.pem(), key_pair.serialize_pem())
}

fn generate_client_cert(
    cn: &str,
    validity: Duration,
    ca_cert: &rcgen::Certificate,
    ca_key: &KeyPair,
) -> (String, String) {
    let key_pair = KeyPair::generate().expect("Failed to generate client key pair");

    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, cn);
    dn.push(DnType::OrganizationName, "Quilt Mesh");
    params.distinguished_name = dn;
    params.is_ca = IsCa::NoCa;
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
    params.not_before = OffsetDateTime::now_utc();
    params.not_after = OffsetDateTime::now_utc() + validity;

    let cert = params
        .signed_by(&key_pair, ca_cert, ca_key)
        .expect("Failed to sign client certificate");

    (cert.pem(), key_pair.serialize_pem())
}

fn write_file(path: &std::path::Path, content: &str) {
    fs::write(path, content).unwrap_or_else(|e| {
        panic!("Failed to write {:?}: {}", path, e);
    });
}
