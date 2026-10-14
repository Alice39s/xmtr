import httpx
import stamina

from xmtr_ci.project import EMAIL

client = httpx.Client(
    headers={"User-Agent": f"xmtr release workflow ({EMAIL})"},
    follow_redirects=True,
    timeout=60,
)


@stamina.retry(on=httpx.TransportError, attempts=5)
def get(url: str) -> httpx.Response:
    return client.get(url)


def download(url: str) -> bytes:
    return get(url).raise_for_status().content


def exists(url: str) -> bool:
    response = get(url)
    if response.status_code == httpx.codes.NOT_FOUND:
        return False
    response.raise_for_status()
    return True
