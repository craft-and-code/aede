use super::*;
use crate::accounts_test_support::Fixture;

const NOW: u64 = 20_000 * 86_400 + 12 * 3600;

struct Controlled {
    _fixture: Fixture,
    state: ApiState,
    session: Session,
}
impl Controlled {
    fn new(age: u64, limit: Option<u32>) -> Self {
        let fixture = Fixture::new();
        let accounts = fixture.accounts();
        let account = accounts.find("alice").unwrap();
        let (captured, _) = auth::start_session(&fixture.0, &accounts, account).unwrap();
        let mut policy = aede_core::parental::Policy {
            enabled: true,
            ..Default::default()
        };
        for day in &mut policy.days {
            day.limit_seconds = limit;
        }
        let mut data = ParentalStore::default();
        let snapshot = data.set_policy(&account.id, policy, 0).unwrap();
        let grant = data
            .reserve(
                &account.id,
                &"a".repeat(64),
                Media::Music { track: None },
                10,
                NOW - age,
            )
            .unwrap();
        data.save(&aede_core::parental::path(&fixture.0.data_dir))
            .unwrap();
        let mut state = fixture.0.clone();
        state.parental_scope = Some(Scope {
            owner: account.id.clone(),
            revision: snapshot.revision,
            policy: snapshot.policy,
            publication: state.parental.publication.load(Ordering::Acquire),
        });
        let session = Session {
            owner: account.id.clone(),
            captured,
            held: BlockingMutex::new(Some(Held {
                grant,
                base_ms: 0,
                sent_base_ms: 0,
            })),
            observed_ms: AtomicU64::new(0),
            sent_ms: AtomicU64::new(0),
            started: std::sync::OnceLock::new(),
            failure: BlockingMutex::new(None),
            last_checked: BlockingMutex::new(None),
        };
        Self {
            _fixture: fixture,
            state,
            session,
        }
    }
    fn used(&self) -> u64 {
        load(&self.state)
            .unwrap()
            .status(&self.session.owner, NOW, &Media::Music { track: None })
            .unwrap()
            .day_used_seconds
    }
    fn refresh(&self, close: bool) {
        self.session
            .refresh_with_clock(&self.state, close, || NOW)
            .unwrap();
    }
}

#[test]
fn closing_without_acknowledgements_does_not_refund_transmitted_pcm() {
    let fixture = Controlled::new(0, Some(2));
    fixture.session.sent(1000);
    fixture.refresh(true);
    assert_eq!(fixture.used(), 1);
}

#[test]
fn closing_acknowledged_pcm_charges_the_audio_once() {
    let fixture = Controlled::new(0, None);
    fixture.session.sent(1000);
    fixture.session.observe(1000);
    fixture.refresh(true);
    assert_eq!(fixture.used(), 1);
}

#[test]
fn acknowledgements_for_the_previous_grant_are_not_charged_again_after_renewal() {
    let fixture = Controlled::new(10, None);
    fixture.session.sent(10_000);
    fixture.session.observe(9000);
    fixture.refresh(false);
    fixture.session.observe(10_000);
    fixture.refresh(true);
    assert_eq!(fixture.used(), 10);
}
