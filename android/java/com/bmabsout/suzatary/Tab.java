package com.bmabsout.suzatary;

import android.app.Activity;
import android.content.ActivityNotFoundException;
import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;

/**
 * A page that must open outside the app's WebView, as a Custom Tab where the
 * browser has them. The session extra is the protocol's documented key, with no
 * session; a browser without Custom Tabs opens the page as a plain link.
 */
final class Tab {

    private Tab() {}

    /** Whether a browser took the page; only an https page is opened. */
    static boolean open(Activity activity, Uri page) {
        if (!"https".equals(page.getScheme())) {
            return false;
        }
        Bundle tab = new Bundle();
        tab.putBinder("android.support.customtabs.extra.SESSION", null);
        try {
            activity.startActivity(new Intent(Intent.ACTION_VIEW, page).putExtras(tab));
            return true;
        } catch (ActivityNotFoundException e) {
            return false;
        }
    }
}
