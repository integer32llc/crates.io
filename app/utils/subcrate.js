export default function sanitizeSubcrateIdForUrl(id) {
    if (id) {
        return id.replaceAll("/", "~");
    } else {
        return id;
    }
}
