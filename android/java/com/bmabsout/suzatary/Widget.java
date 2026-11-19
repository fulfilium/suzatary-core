/*
 * THE HOME SCREEN, SHOWING WHAT IS MOST OWED.
 *
 * A WIDGET IS A RENDERING OF THE BOX'S FOLD AND NOTHING ELSE. Every number
 * below came out of `GET /api/fold?limit=5` as a string and is put on the
 * screen as that string; every line of text is `Authored.body` as the chain
 * stores it. This class holds no fold, no FPL interpreter, no spec, no clock
 * arithmetic and no idea what "fulfillment" means — which is the same rule
 * MainActivity states for the WebView (no fold, no fulfillment, no verifier)
 * one surface further out, and the rule this repository has already enforced
 * twice by deleting a second evaluator.
 *
 * THE NUMBER IS NOT TURNED INTO A PERCENTAGE HERE. `value * 100` looks like
 * formatting and is arithmetic on a fulfillment, and the moment this file does
 * one piece of that it is a place where the number on a phone can disagree
 * with the number on the box. The web app has ONE renderer for a fulfillment
 * (`web/src/view/format.ts`), it is in TypeScript, and there is nothing on the box
 * to reuse from Java — so this prints what the JSON spells, and the reader who
 * wants it as a percentage taps the row and gets the app.
 *
 * IDENTITY IS THE TAILNET'S, exactly as for the WebView. The request carries no
 * credential and no token: `tailscale whois` names the phone's login, so a fold
 * this widget can read is a fold a browser tab on the same phone can read, and
 * removing the device from the tailnet takes both. It sends NO
 * `X-Suzatary-Act` header, because that header is the CSRF half of a WRITE
 * (`gate::check_csrf`) and this widget never writes anything at all. HTTPS
 * only, like Push and the WebView: a widget
 * that cannot reach the box shows the rows it last saw and says how old they
 * are, which is a better answer than downgrading a connection nobody is
 * watching.
 *
 * NOTHING OF OURS RUNS BETWEEN TWO UPDATES. This is a BroadcastReceiver, like
 * PushReceiver: the system wakes the process, the fetch happens on a thread
 * held open by goAsync, the rows are handed to the launcher and the process
 * goes away. No service, no job, no alarm — the same requirement push was
 * built to satisfy, and the reason updatePeriodMillis (res/xml/widget_info.xml)
 * is the system's batched wake-up rather than a timer of ours.
 *
 * EVERY ANDROID DETAIL BELOW IS REVERSE-DOCUMENTED OR DOCUMENTED, and each one
 * says which and how confident, the way Push.java does for the C2DM protocol.
 */
package com.bmabsout.suzatary;

import android.app.PendingIntent;
import android.appwidget.AppWidgetManager;
import android.appwidget.AppWidgetProvider;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.net.Uri;
import android.util.Log;
import android.view.View;
import android.widget.RemoteViews;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.net.URL;
import java.util.ArrayList;
import java.util.Date;
import java.util.List;

import javax.net.ssl.HttpsURLConnection;

public final class Widget extends AppWidgetProvider {

    private static final String TAG = "SuzataryWidget";

    /**
     * How many rows. FIVE, and the number is spent in the QUERY rather than in
     * a loop here: the box orders the fold and cuts it (`routes::most_owed`),
     * so the phone never holds a list it is about to throw away and never
     * decides which five those are.
     */
    private static final int ROWS = 5;

    /**
     * The fold this widget draws, a path under the box's home page.
     *
     * `limit` is `suzatary_api::query::fold_limit` and it means "the lowest
     * fulfillments, ascending, open only" — pinned by `app/cli/tests/reads.rs`
     * so this widget's contract is a test on the box and not a hope here.
     */
    private static final String ENDPOINT = "api/fold?limit=" + ROWS;

    /** A five-entry fold is a few kilobytes. Anything past this is not one. */
    private static final int MAX_BODY = 256 * 1024;

    /** One line of a todo, on a home screen. Cut, not wrapped. */
    private static final int MAX_LINE = 160;

    private static final String PREFS = "widget";
    private static final String KEY_FOLD = "fold";
    private static final String KEY_AT = "fetched_at";

    /**
     * ASK THE WIDGETS TO REDRAW. Called from PushReceiver when the box wakes
     * this app: a push means the chain moved, and the chain moving is exactly
     * when the five most-owed todos are wrong.
     *
     * A BROADCAST AND NOT A FETCH. This runs on PushReceiver's main thread and
     * must not touch the network; sending the update intent puts the work back
     * through onUpdate below, which already knows how to hold the process open
     * for it. The component is NAMED, so the intent reaches this receiver even
     * though the manifest does not export it — an explicit intent from our own
     * package needs no export. CONFIDENCE: HIGH, documented.
     *
     * Nothing is sent when no widget is on a home screen: `getAppWidgetIds`
     * answers the empty array, and waking a process to draw nothing is the
     * cost this whole design exists to avoid.
     */
    static void refresh(Context context) {
        try {
            AppWidgetManager manager = AppWidgetManager.getInstance(context);
            ComponentName us = new ComponentName(context, Widget.class);
            int[] ids = manager.getAppWidgetIds(us);
            if (ids == null || ids.length == 0) {
                return;
            }
            context.sendBroadcast(
                    new Intent(AppWidgetManager.ACTION_APPWIDGET_UPDATE)
                            .setComponent(us)
                            .putExtra(AppWidgetManager.EXTRA_APPWIDGET_IDS, ids));
        } catch (Throwable e) {
            // A widget that did not refresh is a widget half an hour out of
            // date. A notification that crashed is a notification nobody got.
            Log.w(TAG, "could not ask the widgets to refresh", e);
        }
    }

    /**
     * ⚠️ onUpdate IS THE MAIN THREAD, and the platform kills the process about
     * ten seconds after onReceive returns. goAsync holds it open for the HTTPS
     * read and finish() releases it — the same one place PushReceiver is
     * allowed to outlive a callback, and it ends by construction.
     *
     * goAsync is called HERE rather than in an overridden onReceive because
     * AppWidgetProvider.onReceive dispatches to onUpdate synchronously, so this
     * is still inside the onReceive the platform requires it to be called
     * during. CONFIDENCE: MEDIUM-HIGH — the requirement is documented on
     * BroadcastReceiver; that AppWidgetProvider's dispatch is synchronous is
     * read off its source rather than promised by a spec. If it ever were not,
     * goAsync would throw IllegalStateException and the catch below would log
     * it rather than crashing the launcher's push.
     */
    @Override
    public void onUpdate(Context context, AppWidgetManager manager, int[] widgets) {
        final PendingResult pending = goAsync();
        final Context app = context.getApplicationContext();
        final int[] ids = widgets.clone();
        new Thread(
                        new Runnable() {
                            @Override
                            public void run() {
                                try {
                                    draw(app, ids);
                                } catch (Throwable e) {
                                    Log.w(TAG, "the widget could not be drawn", e);
                                } finally {
                                    pending.finish();
                                }
                            }
                        },
                        "suzatary-widget")
                .start();
    }

    /**
     * Fetch, or fall back to the last fold this phone saw, and hand the
     * launcher a picture either way.
     *
     * THE CACHE IS THE FAILURE MODE. A widget that showed an error would spend
     * a home screen saying nothing useful; five todos that were true twenty
     * minutes ago plus a line saying so is strictly more information, and it is
     * the same local-first bargain `web/src/app/cache.ts` makes one client in.
     */
    private static void draw(Context context, int[] ids) {
        Box box = Box.of(context);
        if (box == null) {
            RemoteViews views = new RemoteViews(context.getPackageName(), R.layout.widget);
            views.removeAllViews(R.id.widget_rows);
            views.setViewVisibility(R.id.widget_note, View.VISIBLE);
            views.setTextViewText(R.id.widget_note, context.getString(R.string.widget_unpaired));
            views.setOnClickPendingIntent(R.id.widget_heading, launches(context));
            AppWidgetManager.getInstance(context).updateAppWidget(ids, views);
            return;
        }
        SharedPreferences prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
        String payload = "";
        try {
            payload = fetch(box);
        } catch (IOException e) {
            Log.i(TAG, "the box did not answer: " + e.getMessage());
        }
        long at;
        boolean stale;
        if (payload.isEmpty()) {
            payload = prefs.getString(KEY_FOLD, "");
            at = prefs.getLong(KEY_AT, 0L);
            stale = true;
        } else {
            at = System.currentTimeMillis();
            prefs.edit().putString(KEY_FOLD, payload).putLong(KEY_AT, at).apply();
            stale = false;
        }

        List<Row> rows = rowsOf(payload);
        RemoteViews views = new RemoteViews(context.getPackageName(), R.layout.widget);
        // The frame ships with no rows and this is called on every update, so
        // the previous fold's rows are removed rather than appended to.
        views.removeAllViews(R.id.widget_rows);
        for (Row row : rows) {
            views.addView(R.id.widget_rows, row.draw(context, box));
        }
        views.setViewVisibility(R.id.widget_note, rows.isEmpty() || stale ? View.VISIBLE : View.GONE);
        // The three things that can be true, most specific first: this phone
        // has never reached the box, these rows are old, or the list really is
        // empty. "Stale" beats "nothing open" because a cache that happened to
        // be empty is not evidence about today.
        if (payload.isEmpty()) {
            views.setTextViewText(R.id.widget_note, context.getString(R.string.widget_unreachable));
        } else if (stale) {
            views.setTextViewText(R.id.widget_note, context.getString(R.string.widget_stale, when(context, at)));
        } else if (rows.isEmpty()) {
            views.setTextViewText(R.id.widget_note, context.getString(R.string.widget_nothing));
        }
        // The heading opens the whole list, so a widget with no rows on it is
        // still a way into the app.
        views.setOnClickPendingIntent(R.id.widget_heading, opens(context, box.https(), 0));
        AppWidgetManager.getInstance(context).updateAppWidget(ids, views);
    }

    /** When the rows on screen were true, in the phone's own clock format. */
    private static String when(Context context, long at) {
        if (at == 0L) {
            return "?";
        }
        return android.text.format.DateFormat.getTimeFormat(context).format(new Date(at));
    }

    /**
     * One row, as three strings the box sent.
     *
     * `value` IS THE JSON'S OWN SPELLING of `Entry.value` — read back out of
     * the parsed number with String.valueOf and never multiplied, rounded or
     * suffixed. See this file's header for why that is a rule and not an
     * oversight. A todo with no number does not reach here: the box's `limit`
     * drops it, because it is not among the lowest of anything.
     */
    private static final class Row {

        private final String todo;
        private final String value;
        private final String body;

        private Row(String todo, String value, String body) {
            this.todo = todo;
            this.value = value;
            this.body = body;
        }

        /**
         * A TAP OPENS THE TODO through the same deep-link path a notification
         * uses: ACTION_VIEW at `#/todo/<id>`, with the class NAMED rather than
         * left to App Link verification, which needs the site up on 443 and may
         * simply not have happened (PushReceiver says the same). The route is
         * in the fragment, so MainActivity hands the WebView the URI whole and
         * the web app's router reads it — this class never learns what an id is
         * for.
         *
         * addView RATHER THAN addStableView, which is the API 31 replacement
         * and the app's floor is 26. The cost is that a row's view identity is
         * its position, so a redraw with different todos re-binds rather than
         * animating; a widget that redraws twice an hour has nothing to
         * animate. CONFIDENCE: HIGH — both are documented, addStableView's
         * `since` is documented.
         */
        private RemoteViews draw(Context context, Box box) {
            RemoteViews view = new RemoteViews(context.getPackageName(), R.layout.widget_row);
            view.setTextViewText(R.id.widget_row_value, value);
            view.setTextViewText(R.id.widget_row_body, body);
            view.setOnClickPendingIntent(
                    R.id.widget_row,
                    opens(context, box.https() + "#/todo/" + Uri.encode(todo), todo.hashCode()));
            return view;
        }
    }

    /**
     * A PendingIntent that opens one of this app's pages.
     *
     * FLAG_IMMUTABLE because nothing may rewrite the address between here and
     * the tap, and FLAG_UPDATE_CURRENT because the fold moves: yesterday's
     * PendingIntent for the same request code must start pointing at today's
     * todo rather than silently keeping the old extras. The request code is the
     * todo's own hash for the same reason PushReceiver uses the click URL's —
     * two rows must not share one PendingIntent.
     */
    private static PendingIntent opens(Context context, String url, int code) {
        Intent open =
                new Intent(Intent.ACTION_VIEW, Uri.parse(url)).setClass(context, MainActivity.class);
        return PendingIntent.getActivity(
                context, code, open, PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
    }

    /** The app itself, where a phone with no box yet is asked for one. */
    private static PendingIntent launches(Context context) {
        Intent open = new Intent(context, MainActivity.class).setAction(Intent.ACTION_MAIN);
        return PendingIntent.getActivity(context, 0, open, PendingIntent.FLAG_IMMUTABLE);
    }

    /**
     * The fold, parsed with `org.json` — WHICH IS IN THE PLATFORM. No Gson, no
     * Moshi, no Jackson: this APK is a Gradle-free derivation with no Maven
     * (nix/apk.nix), and the one parser Android already ships is the one this
     * app can have. It is also the parser Push.java builds its report with, so
     * there is one JSON library on this side and not two.
     *
     * DEFENSIVE ON EVERY FIELD. `opt*` throughout: a payload that is missing
     * something is a row skipped, never an exception thrown inside a broadcast
     * the launcher is waiting on. A row with no id could not be tapped and is
     * dropped rather than drawn dead.
     */
    private static List<Row> rowsOf(String payload) {
        List<Row> rows = new ArrayList<>();
        if (payload.isEmpty()) {
            return rows;
        }
        try {
            JSONArray entries = new JSONObject(payload).optJSONArray("entries");
            if (entries == null) {
                return rows;
            }
            for (int i = 0; i < entries.length() && rows.size() < ROWS; i++) {
                JSONObject entry = entries.optJSONObject(i);
                if (entry == null) {
                    continue;
                }
                String todo = entry.optString("todo", "");
                if (todo.isEmpty()) {
                    continue;
                }
                JSONObject authored = entry.optJSONObject("authored");
                String body = authored == null ? "" : authored.optString("body", "");
                rows.add(new Row(todo, number(entry), line(body.isEmpty() ? todo : body)));
            }
        } catch (JSONException e) {
            Log.w(TAG, "the fold did not parse; showing what there is", e);
        }
        return rows;
    }

    /**
     * `Entry.value`, as the box printed it. `opt` gives back the Double the
     * tokeniser read and String.valueOf prints it; a null (a todo the chain
     * mentions but never priced) is an empty cell rather than the word "null".
     */
    private static String number(JSONObject entry) {
        Object value = entry.opt("value");
        return value == null || JSONObject.NULL.equals(value) ? "" : String.valueOf(value);
    }

    /**
     * A todo's FIRST LINE, capped.
     *
     * IT IS TYPST MARKUP SOURCE and it is shown as source, backslashes and all.
     * `Authored.rich.body` beside it is typst's HTML, and rendering that would
     * make this app a document renderer — the one thing MainActivity's header
     * says it must never become. Showing the source is exactly what
     * `web/src/rich.ts` falls back to when typst has not compiled a body yet,
     * so this widget is the app's own degraded rendering rather than a new one.
     */
    private static String line(String body) {
        int cut = body.indexOf('\n');
        String first = (cut < 0 ? body : body.substring(0, cut)).trim();
        return first.length() <= MAX_LINE ? first : first.substring(0, MAX_LINE);
    }

    /** The read. Returns "" for anything that is not a 2xx with a body. */
    private static String fetch(Box box) throws IOException {
        HttpsURLConnection connection = (HttpsURLConnection) new URL(box.https() + ENDPOINT).openConnection();
        try {
            connection.setRequestMethod("GET");
            connection.setConnectTimeout(15000);
            connection.setReadTimeout(15000);
            connection.setRequestProperty("Accept", "application/json");
            int code = connection.getResponseCode();
            if (code / 100 != 2) {
                Log.w(TAG, "the box refused the fold: " + code);
                return "";
            }
            return read(connection.getInputStream());
        } finally {
            connection.disconnect();
        }
    }

    /**
     * A capped read of a body this app is about to PARSE — which is why it is
     * not Push.read, whose 512 bytes are a refusal to put in a log. Same idea,
     * different size and different purpose, and sharing one would mean one of
     * the two callers getting the wrong cap.
     */
    private static String read(InputStream in) throws IOException {
        if (in == null) {
            return "";
        }
        try {
            InputStreamReader reader = new InputStreamReader(in, "UTF-8");
            StringBuilder out = new StringBuilder();
            char[] buffer = new char[4096];
            int n;
            while (out.length() < MAX_BODY && (n = reader.read(buffer)) > 0) {
                out.append(buffer, 0, n);
            }
            return out.toString();
        } finally {
            in.close();
        }
    }
}
