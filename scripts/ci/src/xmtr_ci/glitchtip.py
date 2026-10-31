"""Releases on the GlitchTip project that receives xmtr's error and usage reports."""

import httpx
import stamina

API = "https://api-user.xmsl.dev/api/0"
ORGANIZATION = "xmsl-root"
PROJECT = "xmtr-cli-telemetry"


def release_name(version: str) -> str:
    """The release xmtr reports, as `sentry::release_name!()` builds it."""
    return f"xmtr@{version}"


@stamina.retry(on=httpx.TransportError, attempts=5)
def post(client: httpx.Client, path: str, body: dict[str, object]) -> None:
    client.post(path, json=body).raise_for_status()


def release(version: str, commit: str, token: str) -> None:
    """Creates the release, or updates it when it exists, and records its production deploy."""
    name = release_name(version)
    releases = f"/organizations/{ORGANIZATION}/releases/"
    headers = {"Authorization": f"Bearer {token}"}
    with httpx.Client(base_url=API, headers=headers, timeout=60) as client:
        post(client, releases, {"version": name, "ref": commit, "projects": [PROJECT]})
        post(client, f"{releases}{name}/deploys/", {"environment": "production"})
    print(f"GlitchTip release {name} at {commit}")
