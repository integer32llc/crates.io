export default function sanitizeSubcrateIdForUrl(id) {
  return id.replaceAll("/", "~");
}
