"""Redaction utilities that run before hashing or persistence."""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import Any, Mapping


SENSITIVE_KEY_PATTERNS = (
    "password",
    "passwd",
    "secret",
    "auth",
    "token",
    "jwt",
    "session",
    "email",
    "phone",
    "credit",
)

SENSITIVE_TEXT_PATTERNS = {
    "email": re.compile(r"\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b", re.I),
    "phone": re.compile(r"\b(?:\+?1[-.\s]?)?(?:\(?\d{3}\)?[-.\s]?)\d{3}[-.\s]?\d{4}\b"),
    "credit_card": re.compile(r"\b(?:\d[ -]*?){13,19}\b"),
    "jwt": re.compile(r"\beyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b"),
    "token": re.compile(r"\b(?:token|api[_-]?key|bearer)\s*[:=]\s*[A-Za-z0-9._~+/=-]{8,}\b", re.I),
}

TEXT_KEYS = {"text", "label", "contentDescription", "content_description", "hint"}


@dataclass(frozen=True)
class RedactionPolicy:
    commit_verbatim_text: bool = False
    commit_text_hashes: bool = False
    allowed_text_roles: tuple[str, ...] = ()
    allowed_static_text_patterns: tuple[str, ...] = ()
    sensitive_key_patterns: tuple[str, ...] = SENSITIVE_KEY_PATTERNS
    sensitive_text_patterns: Mapping[str, re.Pattern[str]] = field(
        default_factory=lambda: SENSITIVE_TEXT_PATTERNS
    )

    def allows_static_text(self, value: str) -> bool:
        return any(re.fullmatch(pattern, value) for pattern in self.allowed_static_text_patterns)


DEFAULT_POLICY = RedactionPolicy()
REDACTED = "<redacted>"
EXCLUDED_TEXT = "<text-excluded>"


def _is_sensitive_key(key: str, policy: RedactionPolicy) -> bool:
    lowered = key.lower()
    return any(pattern.lower() in lowered for pattern in policy.sensitive_key_patterns)


def _text_class(value: str) -> str:
    stripped = value.strip()
    if not stripped:
        return "empty"
    if stripped.isdigit():
        return "numeric"
    if len(stripped) <= 3:
        return "short"
    if len(stripped) <= 24:
        return "medium"
    return "long"


def redact_text(value: str, policy: RedactionPolicy = DEFAULT_POLICY) -> dict[str, Any]:
    for name, pattern in policy.sensitive_text_patterns.items():
        if pattern.search(value):
            return {"redacted": True, "reason": name, "text_class": _text_class(value)}
    if policy.commit_verbatim_text or policy.allows_static_text(value):
        return {"redacted": False, "value": value, "text_class": _text_class(value)}
    return {"redacted": True, "reason": "verbatim_text_excluded", "text_class": _text_class(value)}


def redact_value(
    value: Any,
    policy: RedactionPolicy = DEFAULT_POLICY,
    *,
    key: str | None = None,
    role: str | None = None,
) -> Any:
    if key and _is_sensitive_key(key, policy):
        return REDACTED

    if isinstance(value, Mapping):
        nested_role = str(value.get("role", role)) if value.get("role", role) is not None else role
        return {
            str(child_key): redact_value(
                child_value,
                policy,
                key=str(child_key),
                role=nested_role,
            )
            for child_key, child_value in value.items()
        }

    if isinstance(value, list):
        return [redact_value(item, policy, role=role) for item in value]

    if isinstance(value, tuple):
        return [redact_value(item, policy, role=role) for item in value]

    if isinstance(value, str):
        if key in TEXT_KEYS:
            if role and role in policy.allowed_text_roles:
                return redact_text(value, RedactionPolicy(commit_verbatim_text=True))
            redacted = redact_text(value, policy)
            if "value" in redacted:
                return redacted["value"]
            return {
                "text_class": redacted["text_class"],
                "redacted": True,
                "reason": redacted["reason"],
            }
        for name, pattern in policy.sensitive_text_patterns.items():
            if pattern.search(value):
                return {"redacted": True, "reason": name}

    return value


def redact_layout(layout: Mapping[str, Any], policy: RedactionPolicy = DEFAULT_POLICY) -> dict[str, Any]:
    """Return a redacted copy suitable for normalization and hashing."""

    redacted = redact_value(layout, policy)
    if not isinstance(redacted, dict):
        raise TypeError("layout redaction must produce an object")
    return redacted


def policy_from_config(config: Mapping[str, Any] | None) -> RedactionPolicy:
    redaction = dict(config or {})
    allowlist = redaction.get("allowlist_static_text", redaction.get("allowed_static_text_patterns", ()))
    return RedactionPolicy(
        commit_verbatim_text=bool(redaction.get("commit_verbatim_text", False)),
        commit_text_hashes=bool(redaction.get("commit_text_hashes", False)),
        allowed_text_roles=tuple(redaction.get("allowed_text_roles", ())),
        allowed_static_text_patterns=tuple(re.escape(text) for text in allowlist),
        sensitive_key_patterns=tuple(
            redaction.get("sensitive_resource_id_patterns", SENSITIVE_KEY_PATTERNS)
        ),
    )
