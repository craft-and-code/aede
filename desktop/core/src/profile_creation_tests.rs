use super::*;
use crate::configuration::test_support::PublicationRendezvous;

fn retain_winner_configuration(host: &mut Host) -> Vec<u8> {
    let folder = host.root.join("retained-music").display().to_string();
    host.configure(18877, vec![folder]).unwrap();
    let mut config = read(&host.root).unwrap();
    config.identity = Some(configuration::Identity::generate().unwrap());
    save(&host.root, &config).unwrap();
    fs::read(host.root.join("server.json")).unwrap()
}

#[test]
fn concurrent_first_creation_preserves_the_exclusive_winners_profile_and_identity() {
    let fixture = Fixture::new();
    fs::create_dir(&fixture.0).unwrap();
    let root = fixture.0.join("profile");
    let rendezvous = PublicationRendezvous::new(&root);
    let first_root = root.clone();
    let first =
        std::thread::spawn(move || Host::new(first_root, PathBuf::from("missing-test-engine")));
    rendezvous.wait_for_creator();
    let second = Host::new(root.clone(), PathBuf::from("missing-test-engine"));
    // With the original ordering B owns the lock while A has not renamed its
    // default profile. With ownership acquired first, B must refuse instead.
    let (mut winner, retained) = match second {
        Ok(mut host) => {
            let retained = retain_winner_configuration(&mut host);
            rendezvous.release();
            assert!(first.join().unwrap().is_err());
            (host, retained)
        }
        Err(_) => {
            rendezvous.release();
            let mut host = first.join().unwrap().unwrap();
            let retained = retain_winner_configuration(&mut host);
            (host, retained)
        }
    };
    assert_eq!(fs::read(root.join("server.json")).unwrap(), retained);
    let password = winner.password().unwrap();
    assert!(Host::new(root.clone(), PathBuf::new()).is_err());
    assert_eq!(fs::read(root.join("server.json")).unwrap(), retained);
    let status = winner.status().unwrap();
    assert_eq!(status.port, 18877);
    assert_eq!(status.roots.len(), 1);
    drop(winner);
    let mut restarted = Host::new(root.clone(), PathBuf::new()).unwrap();
    assert_eq!(restarted.password().unwrap(), password);
    assert_eq!(restarted.status().unwrap().port, 18877);
    assert_eq!(fs::read(root.join("server.json")).unwrap(), retained);
}
