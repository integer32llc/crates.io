const SUBCRATE_DELIMETER = "/";
const SUBCRATE_DELIMETER_FILENAME_REPLACEMENT = "~";

export function sanitizeSubcrateIdForUrl(id) {
    if (id) {
        return id.replaceAll(SUBCRATE_DELIMETER, SUBCRATE_DELIMETER_FILENAME_REPLACEMENT);
    } else {
        return id;
    }
}

export function decodeSubcrateIdFromUrl(id) {
    if (id) {
        return id.replaceAll(SUBCRATE_DELIMETER_FILENAME_REPLACEMENT, SUBCRATE_DELIMETER);
    } else {
        return id;
    }
}
