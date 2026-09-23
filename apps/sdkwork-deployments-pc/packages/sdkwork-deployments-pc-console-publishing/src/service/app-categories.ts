/**
 * Multi-level application category taxonomy for the create-deploy-app dialog.
 *
 * The deploy schema keeps no category table; per the sdkwork-specs
 * contract (`deploy_app.metadata` is a documented free-form JSONB column) the
 * selection is persisted as `metadata.category = { id, path }`. The taxonomy
 * itself is declarative data so it can be swapped for a server-driven catalog
 * (e.g. the appstore catalog API) without touching the dialog.
 */
import type { AppKind } from "@sdkwork/deployments-pc-console-core/sdk";
import type { PublishingMessageKey } from "../i18n.ts";

/** One selectable category node. */
export interface DeployAppCategoryNode {
  readonly id: string
  /** Locale key for the display label. */
  readonly labelKey: PublishingMessageKey
  /** App kinds this subtree applies to; empty means all kinds. */
  readonly appKinds?: readonly AppKind[] | undefined
  readonly children?: readonly DeployAppCategoryNode[] | undefined
}

/** Full taxonomy: up to three levels per top category. */
export const DEPLOY_APP_CATEGORY_TREE: readonly DeployAppCategoryNode[] = [
  {
    id: "business",
    labelKey: "catBusiness",
    children: [
      {
        id: "business-finance",
        labelKey: "catBusinessFinance",
        children: [
          { id: "business-finance-trading", labelKey: "catBusinessFinanceTrading" },
          { id: "business-finance-payment", labelKey: "catBusinessFinancePayment" },
        ],
      },
      {
        id: "business-office",
        labelKey: "catBusinessOffice",
        children: [
          { id: "business-office-documents", labelKey: "catBusinessOfficeDocuments" },
          { id: "business-office-collaboration", labelKey: "catBusinessOfficeCollaboration" },
        ],
      },
      {
        id: "business-ecommerce",
        labelKey: "catBusinessEcommerce",
        children: [
          { id: "business-ecommerce-shopping", labelKey: "catBusinessEcommerceShopping" },
          { id: "business-ecommerce-logistics", labelKey: "catBusinessEcommerceLogistics" },
        ],
      },
      {
        id: "business-enterprise",
        labelKey: "catBusinessEnterprise",
        children: [
          { id: "business-enterprise-erp", labelKey: "catBusinessEnterpriseErp" },
          { id: "business-enterprise-hr", labelKey: "catBusinessEnterpriseHr" },
        ],
      },
    ],
  },
  {
    id: "developer",
    labelKey: "catDev",
    children: [
      {
        id: "dev-tools",
        labelKey: "catDevTools",
        children: [
          { id: "dev-tools-ide", labelKey: "catDevToolsIde" },
          { id: "dev-tools-cicd", labelKey: "catDevToolsCicd" },
          { id: "dev-tools-database", labelKey: "catDevToolsDatabase" },
        ],
      },
      {
        id: "dev-apis",
        labelKey: "catDevApis",
        children: [
          { id: "dev-apis-gateway", labelKey: "catDevApisGateway" },
          { id: "dev-apis-sdk", labelKey: "catDevApisSdk" },
        ],
      },
      {
        id: "dev-ai",
        labelKey: "catDevAi",
        children: [
          { id: "dev-ai-llms", labelKey: "catDevAiLlms" },
          { id: "dev-ai-mlops", labelKey: "catDevAiMlops" },
        ],
      },
    ],
  },
  {
    id: "education",
    labelKey: "catEducation",
    children: [
      {
        id: "education-learning",
        labelKey: "catEducationLearning",
        children: [
          { id: "education-learning-courses", labelKey: "catEducationLearningCourses" },
          { id: "education-learning-languages", labelKey: "catEducationLearningLanguages" },
        ],
      },
      {
        id: "education-study",
        labelKey: "catEducationStudy",
        children: [
          { id: "education-study-exams", labelKey: "catEducationStudyExams" },
        ],
      },
    ],
  },
  {
    id: "entertainment",
    labelKey: "catEntertainment",
    children: [
      {
        id: "entertainment-video",
        labelKey: "catEntertainmentVideo",
        children: [
          { id: "entertainment-video-streaming", labelKey: "catEntertainmentVideoStreaming" },
          { id: "entertainment-video-short", labelKey: "catEntertainmentVideoShort" },
        ],
      },
      {
        id: "entertainment-audio",
        labelKey: "catEntertainmentAudio",
        children: [
          { id: "entertainment-audio-music", labelKey: "catEntertainmentAudioMusic" },
          { id: "entertainment-audio-podcasts", labelKey: "catEntertainmentAudioPodcasts" },
        ],
      },
      {
        id: "entertainment-games",
        labelKey: "catEntertainmentGames",
        children: [
          { id: "entertainment-games-casual", labelKey: "catEntertainmentGamesCasual" },
          { id: "entertainment-games-puzzle", labelKey: "catEntertainmentGamesPuzzle" },
          { id: "entertainment-games-strategy", labelKey: "catEntertainmentGamesStrategy" },
        ],
      },
    ],
  },
  {
    id: "lifestyle",
    labelKey: "catLife",
    children: [
      {
        id: "life-health",
        labelKey: "catLifeHealth",
        children: [
          { id: "life-health-fitness", labelKey: "catLifeHealthFitness" },
          { id: "life-health-medical", labelKey: "catLifeHealthMedical" },
        ],
      },
      {
        id: "life-food",
        labelKey: "catLifeFood",
        children: [
          { id: "life-food-recipe", labelKey: "catLifeFoodRecipe" },
          { id: "life-food-delivery", labelKey: "catLifeFoodDelivery" },
        ],
      },
      {
        id: "life-travel",
        labelKey: "catLifeTravel",
        children: [
          { id: "life-travel-booking", labelKey: "catLifeTravelBooking" },
          { id: "life-travel-maps", labelKey: "catLifeTravelMaps" },
        ],
      },
      {
        id: "life-social",
        labelKey: "catLifeSocial",
        children: [
          { id: "life-social-messaging", labelKey: "catLifeSocialMessaging" },
          { id: "life-social-community", labelKey: "catLifeSocialCommunity" },
          { id: "life-social-live", labelKey: "catLifeSocialLive" },
        ],
      },
    ],
  },
  {
    id: "news",
    labelKey: "catNews",
    children: [
      {
        id: "news-news",
        labelKey: "catNewsNews",
        children: [
          { id: "news-news-media", labelKey: "catNewsNewsMedia" },
          { id: "news-news-finance", labelKey: "catNewsNewsFinance" },
        ],
      },
    ],
  },
  {
    id: "utilities",
    labelKey: "catUtilities",
    children: [
      {
        id: "utilities-system",
        labelKey: "catUtilitiesSystem",
        children: [
          { id: "utilities-system-storage", labelKey: "catUtilitiesSystemStorage" },
          { id: "utilities-system-network", labelKey: "catUtilitiesSystemNetwork" },
        ],
      },
      {
        id: "utilities-productivity",
        labelKey: "catUtilitiesProductivity",
        children: [
          { id: "utilities-productivity-calendar", labelKey: "catUtilitiesProductivityCalendar" },
          { id: "utilities-productivity-notes", labelKey: "catUtilitiesProductivityNotes" },
        ],
      },
    ],
  },
  {
    id: "shopping",
    labelKey: "catShopping",
    children: [
      {
        id: "shopping-retail",
        labelKey: "catShoppingRetail",
        children: [
          { id: "shopping-retail-marketplace", labelKey: "catShoppingRetailMarketplace" },
          { id: "shopping-retail-deals", labelKey: "catShoppingRetailDeals" },
        ],
      },
    ],
  },
] as const;

/** Whether a node (or any descendant) is applicable to the given app kind. */
function nodeApplies(node: DeployAppCategoryNode, appKind: AppKind | undefined): boolean {
  if (appKind === undefined) return true
  if (node.appKinds !== undefined && node.appKinds.length > 0 && !node.appKinds.includes(appKind)) {
    return node.children?.some((child) => nodeApplies(child, appKind)) ?? false
  }
  return true
}

/** Filtered copy of the taxonomy for one app kind (undefined = all kinds). */
export function categoriesForAppKind(
  appKind: AppKind | undefined,
  tree: readonly DeployAppCategoryNode[] = DEPLOY_APP_CATEGORY_TREE,
): readonly DeployAppCategoryNode[] {
  return tree
    .filter((node) => nodeApplies(node, appKind))
    .map((node) => ({
      ...node,
      children: node.children ? categoriesForAppKind(appKind, node.children) : undefined,
    }))
}

/** Resolve a category node by id anywhere in the tree. */
export function findCategoryNode(
  id: string,
  tree: readonly DeployAppCategoryNode[] = DEPLOY_APP_CATEGORY_TREE,
): DeployAppCategoryNode | undefined {
  for (const node of tree) {
    if (node.id === id) return node
    const found = node.children ? findCategoryNode(id, node.children) : undefined
    if (found) return found
  }
  return undefined
}

/** Breadcrumb path labels from a root node down to a leaf id. */
export function categoryPathTo(
  id: string,
  tree: readonly DeployAppCategoryNode[] = DEPLOY_APP_CATEGORY_TREE,
): readonly DeployAppCategoryNode[] {
  for (const node of tree) {
    if (node.id === id) return [node]
    if (node.children) {
      const tail = categoryPathTo(id, node.children)
      if (tail.length > 0) return [node, ...tail]
    }
  }
  return []
}
