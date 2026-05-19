type GatewayState = "running" | "stopped" | "active";

interface GreetingSet {
  en: string[];
  zh: string[];
}

const timeGreetings: Record<string, GreetingSet> = {
  morning: {
    en: ["Good morning!", "Rise and shine!", "Ready to code?"],
    zh: ["早上好！", "新的一天开始了~", "准备好写代码了吗？"],
  },
  afternoon: {
    en: ["Good afternoon!", "Keep it up!", "How's it going?"],
    zh: ["下午好！", "继续加油！", "进展如何？"],
  },
  evening: {
    en: ["Good evening!", "Still at it?", "Don't forget to rest!"],
    zh: ["晚上好！", "还在忙？", "别忘了休息~"],
  },
  night: {
    en: ["Working late?", "Get some sleep!", "Night owl mode!"],
    zh: ["夜深了~", "该休息了！", "夜猫子模式！"],
  },
};

const stateGreetings: Record<GatewayState, GreetingSet> = {
  running: {
    en: ["All systems go!", "Gateway is ready~", "Waiting for requests..."],
    zh: ["一切正常！", "网关待命中~", "等待请求中..."],
  },
  active: {
    en: ["Busy busy!", "Requests flowing!", "On it!"],
    zh: ["好忙好忙！", "请求来了！", "处理中！"],
  },
  stopped: {
    en: ["Zzz... start the gateway?", "Idle mode~", "Bored..."],
    zh: ["Zzz...要启动网关吗？", "闲置中~", "好无聊..."],
  },
};

const funGreetings: GreetingSet = {
  en: [
    "Bugs fixed yet?",
    "You're awesome!",
    "Need more coffee?",
    "Ship it!",
    "One more commit...",
    "LGTM!",
  ],
  zh: [
    "Bug 修完了吗？",
    "你真棒！",
    "要不要来杯咖啡？",
    "发布吧！",
    "再改一个 commit...",
    "代码看起来不错！",
  ],
};

function pick<T>(arr: T[]): T {
  return arr[Math.floor(Math.random() * arr.length)];
}

function getTimePeriod(): string {
  const h = new Date().getHours();
  if (h >= 6 && h < 12) return "morning";
  if (h >= 12 && h < 18) return "afternoon";
  if (h >= 18 && h < 23) return "evening";
  return "night";
}

export function getGreeting(gatewayState: GatewayState, locale: "en" | "zh"): string {
  const roll = Math.random();
  if (roll < 0.35) {
    // Time-based greeting
    const period = getTimePeriod();
    return pick(timeGreetings[period][locale]);
  } else if (roll < 0.7) {
    // State-based greeting
    return pick(stateGreetings[gatewayState][locale]);
  } else {
    // Fun greeting
    return pick(funGreetings[locale]);
  }
}
