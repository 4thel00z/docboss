"""The formats `to()` converts a document or page to, one TypedDict each."""

from typing import Literal, NotRequired, TypedDict

ImagesMode = Literal["reference", "embed", "omit"]


class Text(TypedDict):
    format: Literal["text"]
    list_labels: NotRequired[bool]
    headers_footers: NotRequired[bool]
    notes: NotRequired[bool]
    comments: NotRequired[bool]


class Markdown(TypedDict):
    format: Literal["markdown"]
    images: NotRequired[ImagesMode]
    image_prefix: NotRequired[str]
    title: NotRequired[bool]
    page_breaks: NotRequired[bool]
    headers_footers: NotRequired[bool]
    notes: NotRequired[bool]
    comments: NotRequired[bool]


class Html(TypedDict):
    format: Literal["html"]
    standalone: NotRequired[bool]
    images: NotRequired[ImagesMode]
    image_prefix: NotRequired[str]
    headers_footers: NotRequired[bool]
    notes: NotRequired[bool]
    comments: NotRequired[bool]


class Json(TypedDict):
    format: Literal["json"]
    pretty: NotRequired[bool]


class Png(TypedDict):
    format: Literal["png"]
    scale: NotRequired[float]


class Ppm(TypedDict):
    format: Literal["ppm"]
    scale: NotRequired[float]


class Bmp(TypedDict):
    format: Literal["bmp"]
    scale: NotRequired[float]


class Jpeg(TypedDict):
    format: Literal["jpeg", "jpg"]
    scale: NotRequired[float]
    quality: NotRequired[int]


TextFormat = Text | Markdown | Html | Json
ImageFormat = Png | Ppm | Bmp | Jpeg
Format = TextFormat | ImageFormat
