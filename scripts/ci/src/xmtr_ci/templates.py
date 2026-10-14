from jinja2 import Environment, PackageLoader, StrictUndefined

from xmtr_ci import project

environment = Environment(
    loader=PackageLoader("xmtr_ci"),
    undefined=StrictUndefined,
    keep_trailing_newline=True,
    autoescape=False,
)


def render(name: str, /, **context: object) -> str:
    return environment.get_template(f"{name}.jinja").render(context, project=project)
