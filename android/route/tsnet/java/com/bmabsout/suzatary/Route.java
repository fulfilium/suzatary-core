package com.bmabsout.suzatary;

import android.app.Activity;
import android.graphics.Color;
import android.net.Uri;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.View;
import android.webkit.HttpAuthHandler;
import android.widget.Button;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.TextView;
import androidx.webkit.ProxyConfig;
import androidx.webkit.ProxyController;
import androidx.webkit.WebViewFeature;
import com.bmabsout.suzatary.node.Node;
import com.bmabsout.suzatary.node.Reach;
import java.io.File;
import java.util.concurrent.Executor;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * HOW THE WEBVIEW REACHES THE BOX: a tailnet node of the app's own. The node
 * (android/route/tsnet/node, Tailscale's tsnet, bound by gomobile) joins the
 * tailnet as a device of whoever signs in, so the box's `tailscale whois` names
 * that login and the gate admits this app as it admits the phone today. No
 * Tailscale app is needed, and nothing outside this app reaches the tailnet.
 *
 * The WebView reaches the box through the node's proxy on loopback, set with
 * ProxyController for every WebView of the process. The proxy lives as long as
 * the process, asks a password made when it opened that only this class
 * answers, and tunnels to the box's 443 and nowhere else, so TLS runs from the
 * WebView to the box's own certificate as it does today. While the node is
 * stopped or restarting, a request through it waits for the node; the override
 * never comes off, so the page fails closed rather than finding another way.
 *
 * The node runs while the activity is in the foreground and stops when it
 * leaves, except while Tailscale is asking for a sign-in: the answer arrives at
 * the node, which must be up to hear it while the person is in the browser. Its state (keys, the login) lives under the app's private
 * files and outlives the process.
 *
 * Sign-in happens in a browser (a Custom Tab where the browser has them), never
 * in this app's WebView, so the person's Tailscale credentials never pass
 * through code of ours.
 */
final class Route implements Reach {

    private final Activity activity;
    private final String host;
    private final Runnable reached;
    private final View screen;
    private final TextView title;
    private final TextView body;
    private final Button act;

    /** Node.start and Node.stop block on disk and the network; one thread keeps them in order. */
    private final ExecutorService node = Executors.newSingleThreadExecutor();

    private boolean running;
    /** The node's last word was Login: a sign-in is asked for. */
    private boolean asking;
    private boolean everReached;
    private String user;
    private String password;

    Route(Activity activity, FrameLayout root, String host, Runnable reached) {
        this.activity = activity;
        this.host = host;
        this.reached = reached;

        LinearLayout box = new LinearLayout(activity);
        box.setOrientation(LinearLayout.VERTICAL);
        box.setGravity(Gravity.CENTER);
        box.setBackgroundColor(Color.parseColor("#fbfbfa"));
        int pad = Math.round(24 * activity.getResources().getDisplayMetrics().density);
        box.setPadding(pad, pad, pad, pad);
        title = text(20, "#1a1a1a");
        body = text(15, "#4a4a48");
        body.setPadding(0, pad / 2, 0, pad / 2);
        act = new Button(activity);
        box.addView(title);
        box.addView(body);
        box.addView(act);
        screen = box;
        root.addView(screen, new FrameLayout.LayoutParams(
                FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT));
        show(R.string.tsnet_starting, "", 0, null);
    }

    void open() {
        if (running) {
            return;
        }
        running = true;
        final String dir = new File(activity.getFilesDir(), "tailscale").getPath();
        final Reach reach = this;
        node.execute(new Runnable() {
            @Override
            public void run() {
                try {
                    // Go's runtime takes its temporary directory from the app's.
                    go.Seq.setContext(activity.getApplicationContext());
                    Node.start(dir, "suzatary-portal", host, reach);
                } catch (Exception e) {
                    down(String.valueOf(e.getMessage()));
                }
            }
        });
    }

    void close() {
        if (!running || asking) {
            return;
        }
        running = false;
        node.execute(new Runnable() {
            @Override
            public void run() {
                Node.stop();
            }
        });
    }

    /** The proxy's challenge, answered once per password; any other host's is not ours. */
    boolean answers(HttpAuthHandler handler, String host) {
        if (password == null || !"127.0.0.1".equals(host) || !handler.useHttpAuthUsernamePassword()) {
            return false;
        }
        handler.proceed(user, password);
        return true;
    }

    // The node's news arrives on its own threads; each case is drawn on the main one.

    @Override
    public void pending(final String why) {
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                asking = false;
                if (!everReached) {
                    show(R.string.tsnet_starting, why, 0, null);
                }
            }
        });
    }

    @Override
    public void login(final String url) {
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                asking = true;
                show(R.string.tsnet_login_title, activity.getString(R.string.tsnet_login_body),
                        R.string.tsnet_login_action, new Runnable() {
                            @Override
                            public void run() {
                                signIn(url);
                            }
                        });
            }
        });
    }

    @Override
    public void ready(final String proxy, final String user, final String password) {
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                proxied(proxy, user, password);
            }
        });
    }

    @Override
    public void down(final String why) {
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                asking = false;
                show(R.string.tsnet_down_title, why, R.string.offline_retry, new Runnable() {
                    @Override
                    public void run() {
                        running = false; // Node.start replaces whatever session is left
                        open();
                    }
                });
            }
        });
    }

    /** Every WebView's requests go to the node's proxy; once they do, the page is shown. */
    private void proxied(String proxy, String user, String password) {
        asking = false;
        if (!WebViewFeature.isFeatureSupported(WebViewFeature.PROXY_OVERRIDE)) {
            show(R.string.tsnet_down_title, activity.getString(R.string.tsnet_no_proxy), 0, null);
            return;
        }
        this.user = user;
        this.password = password;
        ProxyConfig config = new ProxyConfig.Builder().addProxyRule("http://" + proxy).build();
        Executor main = new Executor() {
            @Override
            public void execute(Runnable task) {
                activity.runOnUiThread(task);
            }
        };
        ProxyController.getInstance().setProxyOverride(config, main, new Runnable() {
            @Override
            public void run() {
                everReached = true;
                screen.setVisibility(View.GONE);
                reached.run();
            }
        });
    }

    /** Tailscale's sign-in page, in the browser; where none opens it, the screen shows the link. */
    private void signIn(String url) {
        Uri page = Uri.parse(url);
        if (!"https".equals(page.getScheme())) {
            return;
        }
        if (!Tab.open(activity, page)) {
            body.setText(url);
        }
    }

    /** The screen over the page: a title, why, and at most one thing to do. */
    private void show(int heading, CharSequence why, int action, final Runnable then) {
        title.setText(heading);
        body.setText(why);
        act.setVisibility(then == null ? View.GONE : View.VISIBLE);
        if (then != null) {
            act.setText(action);
            act.setOnClickListener(new View.OnClickListener() {
                @Override
                public void onClick(View v) {
                    then.run();
                }
            });
        }
        screen.setVisibility(View.VISIBLE);
    }

    private TextView text(int sp, String color) {
        TextView view = new TextView(activity);
        view.setTextSize(TypedValue.COMPLEX_UNIT_SP, sp);
        view.setTextColor(Color.parseColor(color));
        return view;
    }
}
