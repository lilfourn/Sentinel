import { httpRouter } from "convex/server";
import { httpAction } from "./_generated/server";
import { internal } from "./_generated/api";

const http = httpRouter();

// Environment variable helpers - access via process.env in Convex runtime
function getEnv(key: string): string | undefined {
  return typeof process !== "undefined" ? process.env[key] : undefined;
}

// CORS headers for cross-origin requests from Tauri app
// Restrict to Tauri app origin for security (prevents arbitrary web access)
const corsHeaders = {
  "Access-Control-Allow-Origin": "tauri://localhost",
  "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type, Authorization",
};

// Helper to add CORS headers to response
function withCors(response: Response): Response {
  const newHeaders = new Headers(response.headers);
  Object.entries(corsHeaders).forEach(([key, value]) => {
    newHeaders.set(key, value);
  });
  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers: newHeaders,
  });
}

// OPTIONS handler for CORS preflight
const corsPreflightHandler = httpAction(async () => {
  return new Response(null, {
    status: 204,
    headers: corsHeaders,
  });
});

// =============================================================================
// STRIPE WEBHOOK HANDLER
// =============================================================================

/**
 * Handle Stripe webhook events
 * POST /stripe-webhook
 *
 * Required environment variables:
 * - STRIPE_WEBHOOK_SECRET: Webhook signing secret from Stripe dashboard
 * - STRIPE_PRO_PRICE_ID: Price ID for Pro subscription
 */
http.route({
  path: "/stripe-webhook",
  method: "POST",
  handler: httpAction(async (ctx, request) => {
    // Get raw body for signature verification
    const body = await request.text();
    const signature = request.headers.get("stripe-signature");

    if (!signature) {
      return new Response("Missing stripe-signature header", { status: 400 });
    }

    // Get webhook secret from environment
    const webhookSecret = getEnv("STRIPE_WEBHOOK_SECRET");
    if (!webhookSecret) {
      console.error("STRIPE_WEBHOOK_SECRET not configured");
      return new Response("Webhook not configured", { status: 500 });
    }

    // Verify Stripe signature
    let event: StripeEvent;
    try {
      event = await verifyStripeSignature(body, signature, webhookSecret);
    } catch (err) {
      console.error("Webhook signature verification failed:", err);
      return new Response("Invalid signature", { status: 400 });
    }

    // Check idempotency - skip if already processed
    const logResult = await ctx.runMutation(internal.subscriptions.logWebhookEvent, {
      eventId: event.id,
      eventType: event.type,
      payload: JSON.stringify({
        type: event.type,
        customerId: event.data?.object?.customer,
      }),
      status: "processed",
    });

    if (logResult.alreadyProcessed) {
      console.log(`Event ${event.id} already processed, skipping`);
      return new Response("Event already processed", { status: 200 });
    }

    // Handle different event types
    try {
      switch (event.type) {
        case "checkout.session.completed": {
          // User completed checkout - subscription is now active
          const session = event.data.object as unknown as CheckoutSession;
          console.log(`Checkout completed for customer ${session.customer}`);

          if (session.subscription) {
            // Use client_reference_id to find/create subscription for user
            // This is the Clerk tokenIdentifier passed during checkout creation
            const tokenIdentifier = session.client_reference_id;

            if (tokenIdentifier) {
              await ctx.runMutation(internal.subscriptions.createOrUpdateFromCheckout, {
                tokenIdentifier,
                stripeCustomerId: session.customer as string,
                stripeSubscriptionId: session.subscription as string,
              });
            } else {
              // Fallback to customer ID lookup
              await ctx.runMutation(internal.subscriptions.updateFromWebhook, {
                stripeCustomerId: session.customer as string,
                stripeSubscriptionId: session.subscription as string,
                tier: "pro",
                status: "active",
                cancelAtPeriodEnd: false,
              });
            }
          }
          break;
        }

        case "customer.subscription.created":
        case "customer.subscription.updated": {
          const subscription = event.data.object as unknown as StripeSubscription;
          console.log(`Subscription ${subscription.id} ${event.type.split(".")[2]}`);

          await ctx.runMutation(internal.subscriptions.updateFromWebhook, {
            stripeCustomerId: subscription.customer as string,
            stripeSubscriptionId: subscription.id,
            tier: mapPriceToTier(subscription.items?.data?.[0]?.price?.id),
            status: mapStripeStatus(subscription.status),
            currentPeriodStart: subscription.current_period_start * 1000,
            currentPeriodEnd: subscription.current_period_end * 1000,
            cancelAtPeriodEnd: subscription.cancel_at_period_end ?? false,
          });
          break;
        }

        case "customer.subscription.deleted": {
          const subscription = event.data.object as unknown as StripeSubscription;
          console.log(`Subscription ${subscription.id} deleted`);

          await ctx.runMutation(internal.subscriptions.updateFromWebhook, {
            stripeCustomerId: subscription.customer as string,
            stripeSubscriptionId: subscription.id,
            tier: "free",
            status: "canceled",
            cancelAtPeriodEnd: false,
          });
          break;
        }

        case "invoice.payment_failed": {
          const invoice = event.data.object as unknown as StripeInvoice;
          console.log(`Payment failed for invoice ${invoice.id}`);

          if (invoice.subscription) {
            await ctx.runMutation(internal.subscriptions.updateFromWebhook, {
              stripeCustomerId: invoice.customer as string,
              stripeSubscriptionId: invoice.subscription as string,
              tier: "pro", // Keep pro tier but mark as past_due
              status: "past_due",
              cancelAtPeriodEnd: false,
            });
          }
          break;
        }

        case "invoice.payment_succeeded": {
          const invoice = event.data.object as unknown as StripeInvoice;
          console.log(`Payment succeeded for invoice ${invoice.id}`);

          if (invoice.subscription) {
            // Reactivate subscription after successful payment
            await ctx.runMutation(internal.subscriptions.updateFromWebhook, {
              stripeCustomerId: invoice.customer as string,
              stripeSubscriptionId: invoice.subscription as string,
              tier: "pro",
              status: "active",
              cancelAtPeriodEnd: false,
            });
          }
          break;
        }

        default:
          console.log(`Unhandled event type: ${event.type}`);
      }

      return new Response("OK", { status: 200 });
    } catch (err) {
      console.error("Error processing webhook:", err);

      // Log failure for debugging
      await ctx.runMutation(internal.subscriptions.logWebhookEvent, {
        eventId: event.id,
        eventType: event.type,
        status: "failed",
        errorMessage: err instanceof Error ? err.message : String(err),
      });

      return new Response("Error processing webhook", { status: 500 });
    }
  }),
});

// =============================================================================
// STRIPE CHECKOUT SESSION CREATION
// =============================================================================

/**
 * Create a Stripe checkout session
 * POST /create-checkout
 * Body: { priceId: string }
 *
 * Note: This requires a valid Clerk JWT token for authentication
 */
http.route({
  path: "/create-checkout",
  method: "OPTIONS",
  handler: corsPreflightHandler,
});

http.route({
  path: "/create-checkout",
  method: "POST",
  handler: httpAction(async (ctx, _request) => {
    // Verify authentication
    const identity = await ctx.auth.getUserIdentity();
    if (!identity) {
      return withCors(new Response("Unauthorized", { status: 401 }));
    }

    const stripeSecretKey = getEnv("STRIPE_SECRET_KEY");
    if (!stripeSecretKey) {
      return withCors(new Response("Stripe not configured", { status: 500 }));
    }

    const proPriceId = getEnv("STRIPE_PRO_PRICE_ID");
    if (!proPriceId) {
      return withCors(new Response("Price not configured", { status: 500 }));
    }

    try {
      // Create Stripe checkout session
      const response = await fetch("https://api.stripe.com/v1/checkout/sessions", {
        method: "POST",
        headers: {
          Authorization: `Bearer ${stripeSecretKey}`,
          "Content-Type": "application/x-www-form-urlencoded",
        },
        body: new URLSearchParams({
          mode: "subscription",
          "line_items[0][price]": proPriceId,
          "line_items[0][quantity]": "1",
          success_url: getEnv("STRIPE_SUCCESS_URL") || "https://sentinel.app/success",
          cancel_url: getEnv("STRIPE_CANCEL_URL") || "https://sentinel.app/cancel",
          customer_email: identity.email ?? "",
          client_reference_id: identity.tokenIdentifier,
          "metadata[clerk_user_id]": identity.tokenIdentifier,
        }),
      });

      if (!response.ok) {
        const error = await response.text();
        console.error("Stripe error:", error);
        return withCors(new Response("Failed to create checkout session", { status: 500 }));
      }

      const session = await response.json();

      return withCors(new Response(JSON.stringify({ url: session.url }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }));
    } catch (err) {
      console.error("Error creating checkout session:", err);
      return withCors(new Response("Internal error", { status: 500 }));
    }
  }),
});

/**
 * Create a Stripe billing portal session
 * POST /create-portal
 * Body: { customerId: string }
 */
http.route({
  path: "/create-portal",
  method: "OPTIONS",
  handler: corsPreflightHandler,
});

http.route({
  path: "/create-portal",
  method: "POST",
  handler: httpAction(async (ctx, request) => {
    // Verify authentication
    const identity = await ctx.auth.getUserIdentity();
    if (!identity) {
      return withCors(new Response("Unauthorized", { status: 401 }));
    }

    const stripeSecretKey = getEnv("STRIPE_SECRET_KEY");
    if (!stripeSecretKey) {
      return withCors(new Response("Stripe not configured", { status: 500 }));
    }

    try {
      const body = await request.json();
      const { customerId } = body as { customerId: string };

      if (!customerId) {
        return withCors(new Response("Missing customerId", { status: 400 }));
      }

      // SECURITY: Verify the customerId belongs to the authenticated user
      const subscription = await ctx.runQuery(internal.subscriptions.getByTokenIdentifier, {
        tokenIdentifier: identity.tokenIdentifier,
      });

      if (!subscription || subscription.stripeCustomerId !== customerId) {
        console.error(`Customer ID mismatch: requested ${customerId}, user has ${subscription?.stripeCustomerId}`);
        return withCors(new Response("Unauthorized: customer ID does not belong to user", { status: 403 }));
      }

      // Create Stripe portal session
      const response = await fetch("https://api.stripe.com/v1/billing_portal/sessions", {
        method: "POST",
        headers: {
          Authorization: `Bearer ${stripeSecretKey}`,
          "Content-Type": "application/x-www-form-urlencoded",
        },
        body: new URLSearchParams({
          customer: customerId,
          return_url: getEnv("STRIPE_RETURN_URL") || "https://sentinel.app/settings",
        }),
      });

      if (!response.ok) {
        const error = await response.text();
        console.error("Stripe error:", error);
        return withCors(new Response("Failed to create portal session", { status: 500 }));
      }

      const session = await response.json();

      return withCors(new Response(JSON.stringify({ url: session.url }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }));
    } catch (err) {
      console.error("Error creating portal session:", err);
      return withCors(new Response("Internal error", { status: 500 }));
    }
  }),
});

// =============================================================================
// USAGE RECORDING (Fallback for expired JWT)
// =============================================================================

/**
 * Record API usage when JWT is expired
 * POST /record-usage
 * Body: { clerkUserId, model, isExtendedThinking?, requestType? }
 *
 * This endpoint is called when the frontend's JWT has expired but we still
 * need to record usage. It requires the request to come from the Tauri app
 * (verified via CORS) and the clerkUserId must match a user in our database.
 *
 * Note: This is less secure than JWT auth but necessary for desktop apps where
 * JWT refresh isn't always possible. CORS restricts access to tauri://localhost.
 */
http.route({
  path: "/record-usage",
  method: "OPTIONS",
  handler: corsPreflightHandler,
});

http.route({
  path: "/record-usage",
  method: "POST",
  handler: httpAction(async (ctx, request) => {
    // First try JWT auth (preferred)
    const identity = await ctx.auth.getUserIdentity();

    try {
      const body = await request.json() as {
        clerkUserId?: string;
        model?: string;
        isExtendedThinking?: boolean;
        requestType?: string;
      };

      // Validate required fields
      if (!body.model) {
        return withCors(new Response(JSON.stringify({ error: "Missing model" }), {
          status: 400,
          headers: { "Content-Type": "application/json" },
        }));
      }

      // Validate model is one of the allowed values
      const allowedModels = ["haiku", "sonnet", "opus", "gpt52", "gpt5mini", "gpt5nano"];
      if (!allowedModels.includes(body.model)) {
        return withCors(new Response(JSON.stringify({ error: "Invalid model" }), {
          status: 400,
          headers: { "Content-Type": "application/json" },
        }));
      }

      // Validate requestType if provided
      const allowedRequestTypes = ["chat", "organize", "rename"];
      if (body.requestType && !allowedRequestTypes.includes(body.requestType)) {
        return withCors(new Response(JSON.stringify({ error: "Invalid requestType" }), {
          status: 400,
          headers: { "Content-Type": "application/json" },
        }));
      }

      // If we have JWT identity, use the authenticated mutation
      if (identity) {
        // Find user and record usage via standard mutation path
        const user = await ctx.runQuery(internal.users.getUserByToken, {
          tokenIdentifier: identity.tokenIdentifier,
        });

        if (user) {
          await ctx.runMutation(internal.subscriptions.recordUsageByClerkId, {
            clerkUserId: user.clerkId || extractClerkIdFromToken(identity.tokenIdentifier),
            model: body.model as "haiku" | "sonnet" | "opus" | "gpt52" | "gpt5mini" | "gpt5nano",
            isExtendedThinking: body.isExtendedThinking,
            requestType: body.requestType as "chat" | "organize" | "rename" | undefined,
          });

          return withCors(new Response(JSON.stringify({ success: true, method: "jwt" }), {
            status: 200,
            headers: { "Content-Type": "application/json" },
          }));
        }
      }

      // Fallback: Use clerkUserId from request body
      // This is only allowed because CORS restricts access to tauri://localhost
      if (!body.clerkUserId) {
        return withCors(new Response(JSON.stringify({ error: "Authentication required" }), {
          status: 401,
          headers: { "Content-Type": "application/json" },
        }));
      }

      // Validate clerkUserId format
      if (!body.clerkUserId.startsWith("user_")) {
        return withCors(new Response(JSON.stringify({ error: "Invalid request" }), {
          status: 400,
          headers: { "Content-Type": "application/json" },
        }));
      }

      // Record usage using internal mutation
      await ctx.runMutation(internal.subscriptions.recordUsageByClerkId, {
        clerkUserId: body.clerkUserId,
        model: body.model as "haiku" | "sonnet" | "opus" | "gpt52" | "gpt5mini" | "gpt5nano",
        isExtendedThinking: body.isExtendedThinking,
        requestType: body.requestType as "chat" | "organize" | "rename" | undefined,
      });

      return withCors(new Response(JSON.stringify({ success: true, method: "clerkId" }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }));

    } catch (err) {
      console.error("Error recording usage:", err);
      return withCors(new Response(JSON.stringify({ error: "Failed to record usage" }), {
        status: 500,
        headers: { "Content-Type": "application/json" },
      }));
    }
  }),
});

/**
 * Extract Clerk user ID from token identifier
 */
function extractClerkIdFromToken(tokenIdentifier: string): string {
  const parts = tokenIdentifier.split("|");
  return parts[parts.length - 1] || "";
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

// Type definitions for Stripe objects
interface StripeEvent {
  id: string;
  type: string;
  data: {
    object: Record<string, unknown>;
  };
}

interface CheckoutSession {
  customer: string;
  subscription: string | null;
  client_reference_id: string | null;
}

interface StripeSubscription {
  id: string;
  customer: string;
  status: string;
  current_period_start: number;
  current_period_end: number;
  cancel_at_period_end: boolean;
  items: {
    data: Array<{
      price: {
        id: string;
      };
    }>;
  };
}

interface StripeInvoice {
  id: string;
  customer: string;
  subscription: string | null;
}

/**
 * Map Stripe price ID to subscription tier
 */
function mapPriceToTier(priceId: string | undefined): "free" | "pro" {
  const proPriceId = getEnv("STRIPE_PRO_PRICE_ID");
  if (priceId && priceId === proPriceId) {
    return "pro";
  }
  return "free";
}

/**
 * Map Stripe subscription status to our status
 */
function mapStripeStatus(
  status: string
): "active" | "past_due" | "canceled" | "incomplete" | "trialing" {
  const statusMap: Record<
    string,
    "active" | "past_due" | "canceled" | "incomplete" | "trialing"
  > = {
    active: "active",
    past_due: "past_due",
    canceled: "canceled",
    incomplete: "incomplete",
    incomplete_expired: "canceled",
    trialing: "trialing",
    unpaid: "past_due",
  };
  return statusMap[status] ?? "incomplete";
}

/**
 * Constant-time string comparison to prevent timing attacks
 * Returns true if strings are equal, false otherwise
 */
function timingSafeEqual(a: string, b: string): boolean {
  if (a.length !== b.length) {
    return false;
  }
  let result = 0;
  for (let i = 0; i < a.length; i++) {
    result |= a.charCodeAt(i) ^ b.charCodeAt(i);
  }
  return result === 0;
}

/**
 * Verify Stripe webhook signature
 * Implements HMAC-SHA256 verification per Stripe's spec
 */
async function verifyStripeSignature(
  payload: string,
  signature: string,
  secret: string
): Promise<StripeEvent> {
  // Parse signature header: t=timestamp,v1=signature
  const parts = signature.split(",").reduce(
    (acc, part) => {
      const [key, value] = part.split("=");
      acc[key] = value;
      return acc;
    },
    {} as Record<string, string>
  );

  const timestamp = parts.t;
  const expectedSignature = parts.v1;

  if (!timestamp || !expectedSignature) {
    throw new Error("Invalid signature format");
  }

  // Check timestamp is within tolerance (5 minutes)
  const now = Math.floor(Date.now() / 1000);
  if (Math.abs(now - parseInt(timestamp)) > 300) {
    throw new Error("Timestamp outside tolerance");
  }

  // Compute expected signature using HMAC-SHA256
  const signedPayload = `${timestamp}.${payload}`;
  const encoder = new TextEncoder();
  const key = await crypto.subtle.importKey(
    "raw",
    encoder.encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"]
  );
  const signatureBytes = await crypto.subtle.sign(
    "HMAC",
    key,
    encoder.encode(signedPayload)
  );
  const computedSignature = Array.from(new Uint8Array(signatureBytes))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");

  // Constant-time comparison to prevent timing attacks
  if (!timingSafeEqual(computedSignature, expectedSignature)) {
    throw new Error("Signature mismatch");
  }

  return JSON.parse(payload) as StripeEvent;
}

export default http;
