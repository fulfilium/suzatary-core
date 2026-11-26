/*
 * SUZATARY ON ANDROID IS A PORTAL, NOT A SECOND CLIENT.
 *
 * The app opens the web client at the box's tailnet address and holds no fold,
 * no fulfillment, no chain verifier and no HTML: each exists once, in the box
 * or the web app, and a Java copy would be a second evaluator.
 *
 * IDENTITY IS THE TAILNET'S. The app carries no credential and no token: the
 * box admits a peer because `tailscale whois` names its login, so this WebView
 * and Chrome on the same phone are the same actor, and removing the device from
 * the tailnet removes the app. HOW the WebView reaches the tailnet is a Route
 * (android/route/).
 *
 * NATIVE ONLY FOR OS INTEGRATION a browser cannot do: deep links, the share
 * target, push and the widget. Each is a shell over the same HTTP API. A link
 * or a share is handed to the web app as an address (the route is in the
 * fragment, which no intent filter reads and the box never sees), and this
 * class parses none of it into a todo, a route or a fetch.
 *
 * THE BOX IS THE PROFILE'S OR THE PERSON'S (Box). An APK whose profile names a
 * box opens it at once; the generic APK shows the pairing screen first, and
 * keeps the name only once that box has answered Pairing. Until then nothing
 * loads, no route opens and no push is asked for.
 *
 * THIS CLASS AUTHORS NO HTML: the failure screen is Android widgets, so nothing
 * this app composes is parsed as a document.
 */
package com.bmabsout.suzatary;

import android.app.Activity;
import android.content.ActivityNotFoundException;
import android.content.ClipData;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.graphics.Color;
import android.net.Uri;
import android.net.http.SslError;
import android.os.Build;
import android.os.Bundle;
import android.text.InputType;
import android.util.Log;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup.LayoutParams;
import android.webkit.HttpAuthHandler;
import android.webkit.SslErrorHandler;
import android.webkit.ValueCallback;
import android.webkit.WebChromeClient;
import android.webkit.WebMessage;
import android.webkit.WebResourceError;
import android.webkit.WebResourceRequest;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.Button;
import android.widget.EditText;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.TextView;
import android.widget.Toast;

import org.json.JSONException;
import org.json.JSONObject;

import java.io.IOException;

public final class MainActivity extends Activity {

    /**
     * The unverifiable half of the link surface (AndroidManifest.xml):
     * `suzatary://todo/<id>` is the same page as the box's home page plus
     * "#/todo/<id>", for the places that cannot linkify a tailnet name. The mapping lives here
     * and nowhere else, so the scheme cannot come to mean a second thing.
     */
    private static final String SCHEME = "suzatary";

    /**
     * What this WebView adds to its user agent, which the box reads on a Google
     * consent (`google_sign_in::APP_AGENT`): a consent opened from here ends
     * with a link only this app handles, where a browser's ends on the web.
     * A page cannot set a navigation's user agent, and a client that sends this
     * word by hand gains only a redirect for its own callback.
     */
    private static final String AGENT = "SuzataryPortal";

    /** The box's route that sends the owner to Google (`suzatary_google::google::CONSENT`). */
    private static final String CONSENT = "/api/oauth/google/consent/";

    /**
     * The authority of the link the box ends a consent with:
     * `suzatary://google/<card>/<outcome>`. It carries a card and a word, never
     * a code, token, address or account, so opening it by hand shows a page and
     * nothing else.
     */
    private static final String GOOGLE = "google";

    private static final java.util.regex.Pattern CARD = java.util.regex.Pattern.compile("[A-Za-z0-9_-]{1,64}");

    private static final java.util.List<String> OUTCOMES = java.util.Arrays.asList("done", "refused", "not-open");

    /** The add form's address under the box's home page (`web/src/shell/route.ts`). */
    private static final String ADD = "#/add";

    /**
     * How much shared text and subject this app carries into a URL. The overflow
     * is cut, not refused, and the cap stops a hostile app from handing over a
     * megabyte to percent-encode.
     */
    private static final int MAX_SHARED = 1000;

    private static final int MAX_SUBJECT = 200;

    private WebView web;
    private View offline;
    private TextView reason;
    private FrameLayout root;

    /** The box this app opens; null while the generic APK shows the pairing screen. */
    private Box box;

    /** The pairing screen, or null once the app has a box. */
    private View pairing;
    private EditText typed;
    private TextView said;
    private Button connect;

    /** Between onStart and onStop: a box paired then opens its route at once. */
    private boolean foreground;

    /**
     * What load() last asked for, fragment and all. The retry button reloads THIS,
     * since a fragment never reaches the server and so never comes back in an
     * error. A restored WebView does not go through load().
     */
    private String current;

    /** The way to the box (android/route/), opened while the activity is in the foreground. */
    private Route route;

    /**
     * The first page — a link, a restored history, or home — until the route
     * reaches the box; null once it has been shown. Nothing loads before then,
     * since a page asked for over a route that is not up is a page that fails.
     */
    private Runnable first;

    /** The request code of the one file chooser this app starts. */
    private static final int CHOOSE_FILES = 2;

    /**
     * The page waiting on the system picker, or null. It is answered exactly
     * once on every path, cancel included: an unanswered callback leaves the
     * page's file input dead until the WebView is rebuilt.
     */
    private ValueCallback<Uri[]> choosing;

    /** Hands each token Play Services gives to the page; held here, since preferences hold listeners weakly. */
    private final SharedPreferences.OnSharedPreferenceChangeListener tokens =
            new SharedPreferences.OnSharedPreferenceChangeListener() {
                @Override
                public void onSharedPreferenceChanged(SharedPreferences prefs, String key) {
                    if (Push.KEY_TOKEN.equals(key)) {
                        handToken();
                    }
                }
            };

    @Override
    protected void onCreate(final Bundle saved) {
        super.onCreate(saved);

        web = new WebView(this);
        WebSettings s = web.getSettings();
        s.setJavaScriptEnabled(true);
        // localStorage IS the app's offline cache (web/src/app/cache.ts): the last
        // fold, slim and dated. Without DOM storage the app has no local-first
        // half at all and a dropped tailnet is a blank screen.
        s.setDomStorageEnabled(true);
        // Nothing in web/ reads a file or a content:// URI, so the WebView may
        // not either. Narrowing a capability nobody uses costs nothing.
        s.setAllowFileAccess(false);
        s.setAllowContentAccess(false);
        s.setMediaPlaybackRequiresUserGesture(true);
        s.setUserAgentString(s.getUserAgentString() + " " + AGENT);
        web.setWebViewClient(new Portal());
        web.setWebChromeClient(new Chooser());

        offline = buildOfflineView();
        offline.setVisibility(View.GONE);

        root = new FrameLayout(this);
        root.addView(web, new FrameLayout.LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT));
        root.addView(offline, new FrameLayout.LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT));
        setContentView(root);

        box = Box.of(this);
        if (box == null) {
            pairing = buildPairingView();
            root.addView(pairing, new FrameLayout.LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT));
            web.setVisibility(View.GONE);
        } else {
            begin(saved);
        }
        Push.kept(this).registerOnSharedPreferenceChangeListener(tokens);
        askToNotify();
    }

    @Override
    protected void onDestroy() {
        Push.kept(this).unregisterOnSharedPreferenceChangeListener(tokens);
        super.onDestroy();
    }

    /**
     * THE TOKEN GOES TO THE PAGE, which reports it signed: this app holds no
     * device key. postWebMessage delivers only to a main frame at the box's
     * origin, and no JavaScript interface is exposed.
     */
    private void handToken() {
        String token = Push.token(this);
        if (box == null || token == null) {
            return;
        }
        try {
            String said = new JSONObject().put("pushToken", token).toString();
            web.postWebMessage(new WebMessage(said), Uri.parse("https://" + box.host));
        } catch (JSONException e) {
            Log.w(Push.TAG, "the token could not be written for the page", e);
        }
    }

    /** The box is known: the first page waits for the route, and push is asked for. */
    private void begin(final Bundle saved) {
        current = box.https();
        // A link the OS handed us wins over restored state: the process may
        // have been killed while the app sat on some other page, and the thing
        // the owner just tapped is what he asked for. With no link, a restore
        // keeps the page and the scroll; with neither, the app opens home.
        final String target = targetOf(getIntent());
        first = new Runnable() {
            @Override
            public void run() {
                if (target != null) {
                    load(target);
                } else if (saved != null) {
                    web.restoreState(saved);
                } else {
                    load(box.https());
                }
            }
        };
        route = new Route(this, root, box.host, new Runnable() {
            @Override
            public void run() {
                reached();
            }
        });

        // PUSH IS A STARTUP ERRAND, NOT A SERVICE. Opening the app is the only
        // moment this process is reliably alive, so it is when the token is
        // refreshed; between two launches nothing of ours runs at all. Off the
        // main thread because SharedPreferences is disk and startService is
        // IPC, and named so a stray trace says whose thread it was.
        new Thread(new Runnable() {
            @Override
            public void run() {
                Push.ensure(getApplicationContext());
            }
        }, "suzatary-register").start();
    }

    @Override
    protected void onStart() {
        super.onStart();
        foreground = true;
        if (route != null) {
            route.open();
        }
    }

    @Override
    protected void onStop() {
        foreground = false;
        if (route != null) {
            route.close();
        }
        super.onStop();
    }

    /** Ask the typed box off the main thread; it is kept only from Pairing's answer. */
    private void pair() {
        final String host = Box.hostOf(typed.getText());
        if (host == null) {
            said.setText(R.string.pairing_not_a_name);
            return;
        }
        said.setText(getString(R.string.pairing_asking, host));
        connect.setEnabled(false);
        new Thread(new Runnable() {
            @Override
            public void run() {
                Box answered = null;
                String why = "";
                try {
                    answered = Pairing.ask(host);
                    Box.keep(getApplicationContext(), answered);
                } catch (IOException e) {
                    why = String.valueOf(e.getMessage());
                }
                final Box kept = answered;
                final String refused = why;
                runOnUiThread(new Runnable() {
                    @Override
                    public void run() {
                        paired(kept, refused);
                    }
                });
            }
        }, "suzatary-pairing").start();
    }

    /** Pairing's outcome, on the main thread: the box opens, or the screen says why not. */
    private void paired(Box answered, String refused) {
        if (isDestroyed()) {
            return;
        }
        connect.setEnabled(true);
        if (answered == null) {
            said.setText(refused);
            return;
        }
        box = answered;
        root.removeView(pairing);
        pairing = null;
        web.setVisibility(View.VISIBLE);
        begin(null);
        if (foreground) {
            route.open();
        }
    }

    /** The route reached the box; the first page, if it is still waiting, is shown now. */
    private void reached() {
        Runnable page = first;
        first = null;
        if (page != null) {
            page.run();
        }
    }

    /**
     * The runtime half of POST_NOTIFICATIONS (API 33+), asked once per launch
     * while unanswered. The rationale shows before the second ask, which is when
     * shouldShowRequestPermissionRationale turns true. The permission string is
     * spelled out because Manifest.permission only exists from API 33.
     */
    private void askToNotify() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
            return; // below 33 the manifest declaration is the whole of it
        }
        String permission = "android.permission.POST_NOTIFICATIONS";
        if (checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED) {
            return;
        }
        if (shouldShowRequestPermissionRationale(permission)) {
            Toast.makeText(this, R.string.push_rationale, Toast.LENGTH_LONG).show();
        }
        requestPermissions(new String[] {permission}, 1);
    }

    /**
     * A link tapped or a share made while the app runs. singleTask reuses this
     * activity and its WebView; `targetOf` decides what the intent names, for a
     * fresh process and a running one alike.
     */
    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        if (box == null) {
            return; // begin() reads the intent once the box has answered
        }
        final String target = targetOf(intent);
        if (target != null) {
            Runnable page = new Runnable() {
                @Override
                public void run() {
                    load(target);
                }
            };
            if (first != null) {
                first = page; // the route is not up yet
            } else {
                page.run();
            }
        }
    }

    /**
     * The page an incoming intent names, or null if it names none.
     *
     * An https URL for this host is loaded AS GIVEN — the fragment is where
     * `#/todo/<id>` lives and dropping it would land every link on the home
     * list. The custom scheme is rewritten: `suzatary://todo/<id>` is
     * authority "todo" and path "/<id>", which is exactly the route the web
     * app's hash router reads, so the two spellings are one page by
     * construction rather than by a table of cases. Encoded forms are used
     * throughout: an id that needed escaping must stay escaped.
     *
     * Any other host is refused rather than loaded. A filter this app declares
     * is not the only way an intent arrives here, and a WebView pointed at a
     * stranger's page by a stranger's intent is the one thing this class must
     * not do.
     */
    private String targetOf(Intent intent) {
        if (intent == null) {
            return null;
        }
        if (Intent.ACTION_SEND.equals(intent.getAction())) {
            return shared(intent);
        }
        if (!Intent.ACTION_VIEW.equals(intent.getAction())) {
            return null;
        }
        Uri data = intent.getData();
        if (data == null) {
            return null;
        }
        // An http link to this host is read as its https page: the app never asks for cleartext.
        if (box.holds(data)) {
            return over("https", data.toString());
        }
        if (!SCHEME.equals(data.getScheme())) {
            return null;
        }
        if (GOOGLE.equals(data.getAuthority())) {
            return signedIn(data);
        }
        String authority = data.getEncodedAuthority();
        String path = data.getEncodedPath();
        String route = (authority == null ? "" : authority) + (path == null ? "" : path);
        while (route.startsWith("/")) {
            route = route.substring(1); // `suzatary:///todo/x` names the same page
        }
        return route.isEmpty() ? box.https() : box.https() + "#/" + route;
    }

    /**
     * THE END OF A GOOGLE CONSENT: `suzatary://google/<card>/<outcome>` opens the
     * inbox with that card open, where the card shows how the sign-in ended. The
     * outcome word is checked and then not needed, since the card's own state is
     * the answer; a link that is not well formed opens the home page. Nothing in
     * the link is sent anywhere, and the card is encoded into the fragment.
     */
    private String signedIn(Uri data) {
        java.util.List<String> parts = data.getPathSegments();
        if (parts.size() != 2 || !CARD.matcher(parts.get(0)).matches() || !OUTCOMES.contains(parts.get(1))) {
            return box.https();
        }
        return box.https() + "#/inbox?open=" + Uri.encode(parts.get(0));
    }

    /**
     * A share, spelled as the add form's address: EXTRA_TEXT and EXTRA_SUBJECT
     * travel as `#/add` query fields, and the web app decides what they mean.
     * They are read as CharSequence, since styled text is a Spanned that
     * getStringExtra drops. Uri.encode runs once, with no allowed set, because
     * the web side decodes once. A share with nothing in it opens the app.
     */
    private String shared(Intent intent) {
        String text = capped(intent.getCharSequenceExtra(Intent.EXTRA_TEXT), MAX_SHARED);
        String title = capped(intent.getCharSequenceExtra(Intent.EXTRA_SUBJECT), MAX_SUBJECT);
        if (text.isEmpty() && title.isEmpty()) {
            return box.https();
        }
        StringBuilder url = new StringBuilder(box.https()).append(ADD);
        char join = '?';
        if (!text.isEmpty()) {
            url.append(join).append("text=").append(Uri.encode(text));
            join = '&';
        }
        if (!title.isEmpty()) {
            url.append(join).append("title=").append(Uri.encode(title));
        }
        return url.toString();
    }

    /** Remote text, as TEXT, trimmed and cut to a length. Never parsed. */
    private static String capped(CharSequence value, int most) {
        if (value == null) {
            return "";
        }
        String text = value.toString().trim();
        return text.length() <= most ? text : text.substring(0, most);
    }

    @Override
    protected void onSaveInstanceState(Bundle out) {
        super.onSaveInstanceState(out);
        web.saveState(out);
    }

    /** Back walks the app's own history first; at its root it leaves. */
    @Override
    public void onBackPressed() {
        if (pairing != null || offline.getVisibility() == View.VISIBLE) {
            super.onBackPressed();
        } else if (web.canGoBack()) {
            web.goBack();
        } else {
            super.onBackPressed();
        }
    }

    private void load(String url) {
        current = url;
        offline.setVisibility(View.GONE);
        web.setVisibility(View.VISIBLE);
        web.loadUrl(url);
    }

    /** The same page over another scheme. Uri keeps the fragment; string surgery would not. */
    private static String over(String scheme, String url) {
        return Uri.parse(url).buildUpon().scheme(scheme).build().toString();
    }

    /** Show why, in the OS's own words. Never remote content parsed as markup. */
    private void showOffline(CharSequence why) {
        reason.setText(why);
        web.setVisibility(View.GONE);
        offline.setVisibility(View.VISIBLE);
    }

    private View buildOfflineView() {
        LinearLayout screen = new LinearLayout(this);
        screen.setOrientation(LinearLayout.VERTICAL);
        screen.setGravity(Gravity.CENTER);
        screen.setBackgroundColor(Color.parseColor("#fbfbfa")); // the manifest's theme_color
        int pad = dp(24);
        screen.setPadding(pad, pad, pad, pad);

        TextView title = new TextView(this);
        title.setText(R.string.offline_title);
        title.setTextSize(TypedValue.COMPLEX_UNIT_SP, 20);
        title.setTextColor(Color.parseColor("#1a1a1a"));

        TextView body = new TextView(this);
        body.setText(R.string.offline_body);
        body.setTextSize(TypedValue.COMPLEX_UNIT_SP, 15);
        body.setTextColor(Color.parseColor("#4a4a48"));
        body.setPadding(0, dp(12), 0, dp(12));

        reason = new TextView(this);
        reason.setTextSize(TypedValue.COMPLEX_UNIT_SP, 13);
        reason.setTextColor(Color.parseColor("#8a8a86"));

        Button retry = new Button(this);
        retry.setText(R.string.offline_retry);
        retry.setOnClickListener(new View.OnClickListener() {
            @Override
            public void onClick(View v) {
                load(over("https", current)); // the page he was after, not home
            }
        });

        screen.addView(title);
        screen.addView(body);
        screen.addView(reason);
        screen.addView(retry);
        return screen;
    }

    private View buildPairingView() {
        LinearLayout screen = new LinearLayout(this);
        screen.setOrientation(LinearLayout.VERTICAL);
        screen.setGravity(Gravity.CENTER);
        screen.setBackgroundColor(Color.parseColor("#fbfbfa"));
        int pad = dp(24);
        screen.setPadding(pad, pad, pad, pad);

        TextView title = new TextView(this);
        title.setText(R.string.pairing_title);
        title.setTextSize(TypedValue.COMPLEX_UNIT_SP, 20);
        title.setTextColor(Color.parseColor("#1a1a1a"));

        TextView body = new TextView(this);
        body.setText(R.string.pairing_body);
        body.setTextSize(TypedValue.COMPLEX_UNIT_SP, 15);
        body.setTextColor(Color.parseColor("#4a4a48"));
        body.setPadding(0, dp(12), 0, dp(12));

        typed = new EditText(this);
        typed.setHint(R.string.pairing_hint);
        typed.setSingleLine(true);
        typed.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_URI);

        said = new TextView(this);
        said.setTextSize(TypedValue.COMPLEX_UNIT_SP, 13);
        said.setTextColor(Color.parseColor("#8a8a86"));
        said.setPadding(0, dp(8), 0, dp(8));

        connect = new Button(this);
        connect.setText(R.string.pairing_connect);
        connect.setOnClickListener(new View.OnClickListener() {
            @Override
            public void onClick(View v) {
                pair();
            }
        });

        screen.addView(title);
        screen.addView(body);
        screen.addView(typed);
        screen.addView(said);
        screen.addView(connect);
        return screen;
    }

    private int dp(int units) {
        return Math.round(units * getResources().getDisplayMetrics().density);
    }

    /**
     * A WebView opens no picker for `<input type="file">` on its own: without
     * this the composer's attach button did nothing in the app. The system
     * picker hands back content:// grants, so no storage permission is asked.
     */
    private final class Chooser extends WebChromeClient {
        @Override
        public boolean onShowFileChooser(WebView view, ValueCallback<Uri[]> callback, FileChooserParams params) {
            answerChooser(null);
            // Built here rather than by params.createIntent(), which some WebView
            // versions give only the first accept type ("image/*,application/pdf").
            Intent pick = new Intent(Intent.ACTION_GET_CONTENT)
                    .addCategory(Intent.CATEGORY_OPENABLE)
                    .setType("*/*")
                    .putExtra(Intent.EXTRA_MIME_TYPES, accepted(params.getAcceptTypes()))
                    .putExtra(Intent.EXTRA_ALLOW_MULTIPLE, params.getMode() == FileChooserParams.MODE_OPEN_MULTIPLE);
            try {
                startActivityForResult(pick, CHOOSE_FILES);
            } catch (ActivityNotFoundException e) {
                callback.onReceiveValue(null);
                Toast.makeText(MainActivity.this, "No app on this phone picks files", Toast.LENGTH_SHORT).show();
                return true;
            }
            choosing = callback;
            return true;
        }
    }

    /** The input's accept list as MIME types; "" and extensions are dropped, and none means any. */
    private static String[] accepted(String[] accept) {
        java.util.ArrayList<String> types = new java.util.ArrayList<>();
        for (String entry : accept) {
            for (String type : entry.split(",")) {
                if (type.trim().contains("/")) {
                    types.add(type.trim());
                }
            }
        }
        return types.isEmpty() ? new String[] {"*/*"} : types.toArray(new String[0]);
    }

    @Override
    protected void onActivityResult(int request, int result, Intent data) {
        if (request != CHOOSE_FILES) {
            super.onActivityResult(request, result, data);
            return;
        }
        answerChooser(result == RESULT_OK && data != null ? chosen(data) : null);
    }

    private void answerChooser(Uri[] uris) {
        if (choosing != null) {
            choosing.onReceiveValue(uris);
            choosing = null;
        }
    }

    /** Every file the picker returned: several arrive as ClipData, one as the data URI. */
    private static Uri[] chosen(Intent data) {
        ClipData clip = data.getClipData();
        if (clip != null) {
            Uri[] uris = new Uri[clip.getItemCount()];
            for (int i = 0; i < uris.length; i++) {
                uris[i] = clip.getItemAt(i).getUri();
            }
            return uris;
        }
        return data.getData() == null ? null : new Uri[] {data.getData()};
    }

    private final class Portal extends WebViewClient {

        /** The last page asked of the box was its consent route, so the next one outside is Google's. */
        private boolean consenting;

        /**
         * The app's origin stays in the WebView; everything else — a mailto:
         * link in a triaged message, an http link out of a todo — goes to the
         * system, which is where a link to somewhere else belongs.
         */
        @Override
        public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
            Uri url = request.getUrl();
            if (box.host.equals(url.getHost())) {
                String path = url.getPath();
                consenting = path != null && path.startsWith(CONSENT);
                return false;
            }
            // The page the box's consent route redirects to is Google's, which
            // refuses to run in a WebView: it opens as a Custom Tab, and the
            // box's callback brings the owner back by the link above.
            boolean google = consenting;
            consenting = false;
            if (google && Tab.open(MainActivity.this, url)) {
                return true;
            }
            try {
                startActivity(new Intent(Intent.ACTION_VIEW, url).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            } catch (ActivityNotFoundException e) {
                // Nothing on the phone handles the scheme. Staying put is the
                // only honest outcome; swallowing the exception keeps a bad
                // link from killing the app.
            }
            return true;
        }

        /** A box page has loaded and listens: the token goes to it. */
        @Override
        public void onPageFinished(WebView view, String url) {
            if (box != null && box.host.equals(Uri.parse(url).getHost())) {
                handToken();
            }
        }

        /** A challenge only the route can answer (a proxy's); anything else is refused. */
        @Override
        public void onReceivedHttpAuthRequest(WebView view, HttpAuthHandler handler, String host, String realm) {
            if (!route.answers(handler, host)) {
                handler.cancel();
            }
        }

        @Override
        public void onReceivedError(WebView view, WebResourceRequest request, WebResourceError error) {
            if (!request.isForMainFrame()) {
                return; // a missing sub-resource is the page's problem, not ours
            }
            showOffline(error.getDescription() + " — " + request.getUrl());
        }

        /**
         * NEVER proceed(), and NEVER downgrade. An SSL error on the tailnet means
         * the certificate is not the one `tailscale cert` minted, or there is
         * none yet; accepting it, or retrying over plain http, would make this
         * app the weakest reader of a chain whose whole point is that readers
         * check, and would carry the owner's mail in the clear. The offline
         * screen says so, and its retry asks over https again.
         */
        @Override
        public void onReceivedSslError(WebView view, SslErrorHandler handler, SslError error) {
            handler.cancel();
            showOffline("TLS: " + error.getUrl());
        }
    }
}
