"""Screen matching with hash fast path and similarity fallback."""

from __future__ import annotations

from collections import Counter
from dataclasses import dataclass
from typing import Mapping

from .models import ScreenNode
from .normalization import element_key, identity_hash


NORMAL_MATCH_THRESHOLD = 0.78
REPAIR_CANDIDATE_THRESHOLD = 0.65
FAST_MATCH_THRESHOLD = 0.90


@dataclass(frozen=True)
class MatchResult:
    status: str
    screen: ScreenNode | None
    match_confidence: float
    hash_matched: bool = False

    @property
    def is_match(self) -> bool:
        return self.status == "matched"

    @property
    def is_repair_candidate(self) -> bool:
        return self.status == "repair_candidate"


def _weighted_jaccard(left: Counter[str], right: Counter[str]) -> float:
    if not left and not right:
        return 1.0
    keys = set(left) | set(right)
    intersection = sum(min(left[key], right[key]) for key in keys)
    union = sum(max(left[key], right[key]) for key in keys)
    return intersection / union if union else 0.0


def _elements(normalized: Mapping[str, object]) -> list[Mapping[str, object]]:
    elements = normalized.get("elements", [])
    if not isinstance(elements, list):
        return []
    return [element for element in elements if isinstance(element, Mapping)]


def _element_counter(normalized: Mapping[str, object]) -> Counter[str]:
    return Counter(element_key(element) for element in _elements(normalized))


def _path_counter(normalized: Mapping[str, object]) -> Counter[str]:
    return Counter(str(element.get("path", "")) for element in _elements(normalized))


def _role_counter(normalized: Mapping[str, object]) -> Counter[str]:
    distribution = normalized.get("role_distribution")
    if isinstance(distribution, Mapping):
        return Counter({str(key): int(value) for key, value in distribution.items()})
    return Counter(str(element.get("role", "")) for element in _elements(normalized))


def _required_check_pass_rate(screen: ScreenNode, normalized: Mapping[str, object]) -> float:
    required = screen.match_profile.get("required_stable_elements", [])
    if not required:
        return 1.0
    keys = set(_element_counter(normalized))
    passed = 0
    for item in required:
        if not isinstance(item, Mapping):
            continue
        selector = item.get("selector", {})
        value = selector.get("value") if isinstance(selector, Mapping) else None
        role = item.get("role")
        if value and any(str(value) in key for key in keys):
            passed += 1
        elif role and any(key.startswith(f"{role}|") for key in keys):
            passed += 1
    return passed / len(required)


def screen_similarity(
    candidate: Mapping[str, object],
    screen: ScreenNode,
) -> float:
    baseline = screen.normalized or screen.match_profile.get("normalized")
    if not isinstance(baseline, Mapping):
        return 0.0

    element_score = _weighted_jaccard(_element_counter(candidate), _element_counter(baseline))
    check_score = _required_check_pass_rate(screen, candidate)
    structural_score = _weighted_jaccard(_path_counter(candidate), _path_counter(baseline))
    role_score = _weighted_jaccard(_role_counter(candidate), _role_counter(baseline))

    return (
        0.55 * element_score
        + 0.20 * check_score
        + 0.15 * structural_score
        + 0.10 * role_score
    )


def match_screen(
    normalized: Mapping[str, object],
    candidates: list[ScreenNode] | Mapping[str, ScreenNode],
    *,
    threshold: float = NORMAL_MATCH_THRESHOLD,
    repair_threshold: float = REPAIR_CANDIDATE_THRESHOLD,
) -> MatchResult:
    screens = list(candidates.values()) if isinstance(candidates, Mapping) else list(candidates)
    current_hash = identity_hash(normalized)

    for screen in screens:
        if screen.identity_hash == current_hash:
            return MatchResult("matched", screen, 1.0, hash_matched=True)

    best_screen: ScreenNode | None = None
    best_score = 0.0
    for screen in screens:
        score = screen_similarity(normalized, screen)
        if score > best_score:
            best_score = score
            best_screen = screen

    if best_screen is None:
        return MatchResult("screen_unknown", None, 0.0)
    if best_score >= threshold:
        return MatchResult("matched", best_screen, best_score)
    if best_score >= repair_threshold:
        return MatchResult("repair_candidate", best_screen, best_score)
    return MatchResult("screen_unknown", best_screen, best_score)
