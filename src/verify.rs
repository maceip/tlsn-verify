use elliptic_curve::pkcs8::DecodePublicKey;
use p256::ecdsa::{signature::Verifier, VerifyingKey};
use std::time::{Duration, UNIX_EPOCH};

use mpz_core::serialize::CanonicalSerialize;
use tls_core::{
    anchors::{OwnedTrustAnchor, RootCertStore},
    dns::ServerName,
    verify::ServerCertVerifier,
};
use tlsn_core::{proof::SubstringsProof, SessionProof, Signature};

pub fn verify() {
    // Deserialize the proof
    let proof = std::fs::read_to_string("proof.json").unwrap();
    let (session_proof, substrings_proof, domain): (SessionProof, SubstringsProof, String) =
        serde_json::from_str(proof.as_str()).unwrap();

    // Destructure
    let SessionProof {
        header,
        signature,
        handshake_data_decommitment,
    } = session_proof;

    // Notary signature type must be correct
    #[allow(irrefutable_let_patterns)]
    let Signature::P256(signature) = signature.unwrap() else {
        panic!("Notary signature is not P256");
    };

    // Verify the signed header against a trusted Notary's public key
    notary_pubkey()
        .verify(&header.to_bytes(), &signature)
        .unwrap();

    // Verify the decommitment
    handshake_data_decommitment
        .verify(header.handshake_summary().handshake_commitment())
        .unwrap();

    // Verify TLS handshake data. This verifies the server's certificate chain and the server's
    // signature against the provided server name.
    handshake_data_decommitment
        .data()
        .verify(
            &cert_verifier(),
            UNIX_EPOCH + Duration::from_secs(header.handshake_summary().time()),
            &ServerName::try_from(domain.as_str()).unwrap(),
        )
        .unwrap();

    // Verify the proof
    let (sent_slices, recv_slices) = substrings_proof.verify(&header).unwrap();

    // Flatten transcript slices into a bytestring, filling the bytes which the Prover chose not
    // to disclose with 'X'
    let mut transcript_tx = vec![b'X'; header.sent_len() as usize];
    for slice in sent_slices {
        transcript_tx[slice.range()].copy_from_slice(slice.data())
    }

    let mut transcript_rx = vec![b'X'; header.recv_len() as usize];
    for slice in recv_slices {
        transcript_rx[slice.range()].copy_from_slice(slice.data())
    }

    println!("-------------------------------------------------------------------");
    println!(
        "Successfully verified that the bytes below came from a session with {:?}.",
        domain
    );
    println!("Note that the bytes which the Prover chose not to disclose are shown as X.");
    println!();
    println!("Bytes sent:");
    println!();
    print!("{}", String::from_utf8(transcript_tx).unwrap());
    println!();
    println!("Bytes received:");
    println!();
    println!("{}", String::from_utf8(transcript_rx).unwrap());
    println!("-------------------------------------------------------------------");
}

fn cert_verifier() -> impl ServerCertVerifier {
    let mut root_store = RootCertStore::empty();
    root_store.add_server_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.0.iter().map(|ta| {
        OwnedTrustAnchor::from_subject_spki_name_constraints(
            ta.subject,
            ta.spki,
            ta.name_constraints,
        )
    }));

    tls_core::verify::WebPkiVerifier::new(root_store, None)
}

fn notary_pubkey() -> VerifyingKey {
    // from https://github.com/tlsnotary/notary-server/tree/main/src/fixture/notary/notary.key
    // converted with `openssl ec -in notary.key -pubout -outform PEM`

    let pem = "-----BEGIN PUBLIC KEY-----
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEBv36FI4ZFszJa0DQFJ3wWCXvVLFr
cRzMG5kaTeHGoSzDu6cFqx3uEWYpFGo6C0EOUgf+mEgbktLrXocv5yHzKg==
-----END PUBLIC KEY-----";

    VerifyingKey::from_public_key_pem(pem).unwrap()
}