//! One read-only view of every relationship Aède can justify.
//!
//! The catalog contains relationships derived from local tags while the
//! source layer deliberately keeps external relationships attributed and
//! reviewable.  They must not be merged as facts, but exports, annotations and
//! diagnostics need one vocabulary for referring to either kind of edge.  A
//! [`GraphEdge`] is that projection: it keeps provenance and trust while
//! giving the relationship a stable [`RelationRef`].

use crate::model::{Catalog, CreditAttribute, EntityKind};
use crate::sources::{Confidence, ReviewDecision, Side, Sources};
use crate::user::EntityRef;

/// Stable identity of one directed relationship.
///
/// Catalog vector positions never appear here. Endpoints use the same stable
/// references as personal annotations; provenance and the source relationship
/// identifier keep two independent assertions about the same pair distinct.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RelationRef {
    /// Where the directed relationship starts.
    pub source: EntityRef,
    /// Relationship type, such as `performance_of` or `credit:producer`.
    pub kind: String,
    /// Where it ends.
    pub target: EntityRef,
    /// Who asserted or derived it: `tags`, `musicbrainz`, `manual`…
    pub provenance: String,
    /// Stable relationship-row identifier supplied by the source, when any.
    pub source_id: Option<String>,
}

impl RelationRef {
    /// Compact stable selector used by the CLI.
    ///
    /// FNV-1a is only a stable display hash, never security or identity. The
    /// CLI still refuses an ambiguous prefix.
    pub fn id(&self) -> String {
        let mut hash = 0xcbf29ce484222325u64;
        for part in [
            self.source.to_token(),
            self.kind.clone(),
            self.target.to_token(),
            self.provenance.clone(),
            self.source_id.clone().unwrap_or_default(),
        ] {
            for byte in part.bytes().chain(std::iter::once(0)) {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
        }
        format!("{hash:016x}")
    }
}

/// One local or externally sourced relationship ready for display or export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphEdge {
    /// Stable relationship identity.
    pub reference: RelationRef,
    /// Readable name of the source endpoint at projection time.
    pub source_name: String,
    /// Readable name of the target endpoint at projection time.
    pub target_name: String,
    /// Number of local observations summarized by this edge.
    pub weight: u32,
    /// Whether this edge may participate in navigation and queries.
    pub trusted: bool,
    /// Matching confidence for an external assertion.
    pub confidence: Option<Confidence>,
    /// Explicit source-review decision, if one exists.
    pub review: Option<ReviewDecision>,
    /// Time at which an external assertion was fetched.
    pub fetched_at: Option<u64>,
    /// Stable source identifier of the relationship type, when exposed.
    pub relationship_type_id: Option<String>,
    /// Direction supplied by the external source.
    pub direction: Option<String>,
    /// Exact name used on this relationship rather than the canonical name.
    pub credited_as: Option<String>,
    /// Instruments, qualifiers, and other relationship attributes.
    pub attributes: Vec<CreditAttribute>,
    /// Optional start of the relationship's validity period.
    pub began: Option<String>,
    /// Optional end of the relationship's validity period.
    pub ended: Option<String>,
    /// Whether the source explicitly says the relationship has ended.
    pub over: Option<bool>,
    /// Ordering key supplied by the source.
    pub order: Option<u32>,
}

/// Projects local and sourced relationships into one attributed graph view.
pub fn edges(catalog: &Catalog, sources: &Sources) -> Vec<GraphEdge> {
    let mut edges = local_edges(catalog);
    edges.extend(work_edges(catalog, sources));
    edges.extend(credit_edges(catalog, sources));
    edges.extend(membership_edges(catalog, sources));
    edges.sort_by(|left, right| {
        left.reference
            .cmp(&right.reference)
            .then_with(|| left.source_name.cmp(&right.source_name))
            .then_with(|| left.target_name.cmp(&right.target_name))
    });
    edges.dedup_by(|left, right| left.reference == right.reference);
    edges
}

fn local_edges(catalog: &Catalog) -> Vec<GraphEdge> {
    catalog
        .relations
        .iter()
        .filter_map(|relation| {
            let source = EntityRef::of(catalog, relation.source_kind, relation.source_id)?;
            let target = EntityRef::of(catalog, relation.target_kind, relation.target_id)?;
            Some(GraphEdge {
                source_name: source.display_name(catalog),
                target_name: target.display_name(catalog),
                reference: RelationRef {
                    source,
                    kind: relation.kind.clone(),
                    target,
                    provenance: relation.source.clone(),
                    source_id: None,
                },
                weight: relation.weight,
                trusted: true,
                confidence: None,
                review: None,
                fetched_at: None,
                relationship_type_id: None,
                direction: None,
                credited_as: None,
                attributes: Vec::new(),
                began: None,
                ended: None,
                over: None,
                order: None,
            })
        })
        .collect()
}

fn work_edges(catalog: &Catalog, sources: &Sources) -> Vec<GraphEdge> {
    sources
        .work_links(catalog)
        .into_iter()
        .filter_map(|link| {
            let source = EntityRef::of(catalog, EntityKind::Recording, link.recording_id)?;
            let source_name = source.display_name(catalog);
            let target = EntityRef::new(EntityKind::Work, link.work.mbid.clone());
            let target_name = if link.work.title.is_empty() {
                link.work.mbid.clone()
            } else {
                link.work.title.clone()
            };
            Some(GraphEdge {
                reference: RelationRef {
                    source,
                    kind: link
                        .work
                        .relation_type
                        .clone()
                        .unwrap_or_else(|| "performance_of".into()),
                    target,
                    provenance: link.source,
                    source_id: link.work.relation_id.clone(),
                },
                source_name,
                target_name,
                weight: 1,
                trusted: link.trusted,
                confidence: Some(link.confidence),
                review: link.review,
                fetched_at: Some(link.fetched_at),
                relationship_type_id: link.work.relation_type_id.clone(),
                direction: link.work.direction.clone(),
                credited_as: None,
                attributes: link.work.attributes.clone(),
                began: None,
                ended: None,
                over: None,
                order: None,
            })
        })
        .collect()
}

fn credit_edges(catalog: &Catalog, sources: &Sources) -> Vec<GraphEdge> {
    sources
        .credit_links(catalog)
        .into_iter()
        .filter_map(|link| {
            let (source, source_name) =
                artist_endpoint(catalog, &link.credit.artist_mbid, &link.credit.artist_name);
            let (target, target_name) = match &link.work {
                Some(work) => (
                    EntityRef::new(EntityKind::Work, work.mbid.clone()),
                    if work.title.is_empty() {
                        work.mbid.clone()
                    } else {
                        work.title.clone()
                    },
                ),
                None => {
                    let target = EntityRef::of(catalog, EntityKind::Recording, link.recording_id)?;
                    let name = target.display_name(catalog);
                    (target, name)
                }
            };
            Some(GraphEdge {
                reference: RelationRef {
                    source,
                    kind: format!("credit:{}", link.credit.role),
                    target,
                    provenance: link.source,
                    source_id: link.credit.relation_id.clone(),
                },
                source_name,
                target_name,
                weight: 1,
                trusted: link.trusted,
                confidence: Some(link.confidence),
                review: link.review,
                fetched_at: Some(link.fetched_at),
                relationship_type_id: link.credit.role_id.clone(),
                direction: link.credit.direction.clone(),
                credited_as: link.credit.credited_as.clone(),
                attributes: link.credit.attributes.clone(),
                began: link.credit.began.clone(),
                ended: link.credit.ended.clone(),
                over: link.credit.over,
                order: link.credit.order,
            })
        })
        .collect()
}

fn membership_edges(catalog: &Catalog, sources: &Sources) -> Vec<GraphEdge> {
    sources
        .membership_links(catalog)
        .into_iter()
        .filter_map(|link| {
            let local = EntityRef::of(catalog, EntityKind::Artist, link.artist_id)?;
            let local_name = local.display_name(catalog);
            let (related, related_name) = link
                .related_artist_id
                .and_then(|id| {
                    let reference = EntityRef::of(catalog, EntityKind::Artist, id)?;
                    let name = reference.display_name(catalog);
                    Some((reference, name))
                })
                .unwrap_or_else(|| {
                    (
                        EntityRef::new(
                            EntityKind::Artist,
                            format!("mbid:{}", link.membership.mbid),
                        ),
                        link.membership.name.clone(),
                    )
                });
            let (source, source_name, target, target_name) = match link.membership.side {
                Side::Player => (related, related_name, local, local_name),
                Side::Group => (local, local_name, related, related_name),
            };
            Some(GraphEdge {
                reference: RelationRef {
                    source,
                    kind: format!("membership:{}", link.membership.kind),
                    target,
                    provenance: link.source,
                    source_id: link.membership.relation_id.clone(),
                },
                source_name,
                target_name,
                weight: 1,
                trusted: link.trusted,
                confidence: Some(link.confidence),
                review: link.review,
                fetched_at: Some(link.fetched_at),
                relationship_type_id: link.membership.role_id.clone(),
                direction: link.membership.direction.clone(),
                credited_as: link.membership.credited_as.clone(),
                attributes: link
                    .membership
                    .attributes
                    .iter()
                    .map(|name| CreditAttribute {
                        id: None,
                        name: name.clone(),
                        value: None,
                        credited_as: None,
                    })
                    .collect(),
                began: link.membership.began.clone(),
                ended: link.membership.ended.clone(),
                over: link.membership.over,
                order: None,
            })
        })
        .collect()
}

fn artist_endpoint(catalog: &Catalog, mbid: &str, name: &str) -> (EntityRef, String) {
    if let Some(artist) = catalog
        .artists
        .iter()
        .find(|artist| artist.mbid.as_deref() == Some(mbid))
        && let Some(reference) = EntityRef::of(catalog, EntityKind::Artist, artist.id)
    {
        return (reference, artist.name.clone());
    }
    (
        EntityRef::new(EntityKind::Artist, format!("mbid:{mbid}")),
        name.to_string(),
    )
}

/// Finds a relationship by its full ID or an unambiguous prefix.
pub fn select(edges: &[GraphEdge], selector: &str) -> Result<GraphEdge, String> {
    let selector = selector.trim().to_ascii_lowercase();
    if selector.is_empty() {
        return Err("a relation ID is required; run aede relations to list them".into());
    }
    let mut found = edges
        .iter()
        .filter(|edge| edge.reference.id().starts_with(&selector));
    let Some(edge) = found.next() else {
        return Err(format!(
            "no relation starts with \"{selector}\"; run aede relations for the current list"
        ));
    };
    if found.next().is_some() {
        return Err(format!(
            "relation ID \"{selector}\" is ambiguous; copy more characters from aede relations"
        ));
    }
    Ok(edge.clone())
}
