/*
 * PAIRING: the generic APK asks a box it was given how that box wakes the
 * phone, before it keeps the box (`GET /api/portal`, app/api's
 * `wire::PortalPayload`). A 200 with that shape proves only that the gate
 * at this name admits this phone's login at its own `app_url`; it does not
 * prove that the host is a Suzatary box. Off the main thread; the caller
 * draws the outcome.
 */
package com.bmabsout.suzatary;

import org.json.JSONException;
import org.json.JSONObject;

import java.io.IOException;
import java.io.InputStream;
import java.net.URL;
import java.util.regex.Pattern;

import javax.net.ssl.HttpsURLConnection;

final class Pairing {

    /** The answer is a few dozen bytes; anything past this is not one. */
    private static final int MAX_BODY = 4096;

    /** `portal::Sender` on the box: the project's number. */
    private static final Pattern SENDER = Pattern.compile("[0-9]{1,20}");

    private Pairing() {
    }

    /**
     * The box at `host`, as it answered. HTTPS only, like every request this
     * app makes; a refusal or an answer of another shape is an IOException
     * whose message the pairing screen shows.
     */
    static Box ask(String host) throws IOException {
        HttpsURLConnection c = (HttpsURLConnection) new URL("https://" + host + "/api/portal").openConnection();
        try {
            c.setRequestMethod("GET");
            c.setConnectTimeout(15000);
            c.setReadTimeout(15000);
            c.setRequestProperty("Accept", "application/json");
            int code = c.getResponseCode();
            if (code != 200) {
                throw new IOException(host + " answered " + code + ": it is not a box this phone may open");
            }
            return answered(host, read(c.getInputStream()));
        } finally {
            c.disconnect();
        }
    }

    private static Box answered(String host, String body) throws IOException {
        try {
            JSONObject push = new JSONObject(body).getJSONObject("push");
            String kind = push.getString("kind");
            if ("none".equals(kind)) {
                return new Box(host, null);
            }
            String sender = push.getString("sender");
            if ("fcm".equals(kind) && SENDER.matcher(sender).matches()) {
                return new Box(host, sender);
            }
        } catch (JSONException e) {
            throw new IOException(host + " is not a Suzatary box: " + e.getMessage());
        }
        throw new IOException(host + " is not a Suzatary box: its answer names no push this app reads");
    }

    private static String read(InputStream in) throws IOException {
        try {
            byte[] buffer = new byte[MAX_BODY];
            int got = 0;
            int n;
            while (got < buffer.length && (n = in.read(buffer, got, buffer.length - got)) > 0) {
                got += n;
            }
            return new String(buffer, 0, got, "UTF-8");
        } finally {
            in.close();
        }
    }
}
