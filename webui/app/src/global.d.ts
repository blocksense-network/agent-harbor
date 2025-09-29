/// <reference types="vite/client" />

declare global {
  interface Event {
    type: string;
  }

  interface MessageEvent extends Event {
    data: string;
  }

  interface EventSource {
    onmessage: ((event: MessageEvent) => void) | null;
    onerror: ((event: Event) => void) | null;
    onopen: ((event: Event) => void) | null;
    readyState: number;
    url: string;
    close(): void;
  }

  interface KeyboardEvent extends Event {
    key: string;
    ctrlKey: boolean;
    metaKey: boolean;
    shiftKey: boolean;
    preventDefault(): void;
  }

  declare var EventSource: {
    prototype: EventSource;
    new (url: string, eventSourceInitDict?: EventSourceInit): EventSource;
  };

  interface EventSourceInit {
    withCredentials?: boolean;
  }

  interface RequestInit {
    method?: string;
    headers?: Record<string, string>;
    body?: string;
    mode?: string;
    credentials?: string;
    cache?: string;
    redirect?: string;
    referrer?: string;
    referrerPolicy?: string;
    integrity?: string;
    keepalive?: boolean;
    signal?: any;
  }

  interface URLSearchParams {
    append(name: string, value: string): void;
    delete(name: string): void;
    get(name: string): string | null;
    getAll(name: string): string[];
    has(name: string): boolean;
    set(name: string, value: string): void;
    toString(): string;
  }

  declare var URLSearchParams: {
    prototype: URLSearchParams;
    new (
      init?:
        | string
        | URLSearchParams
        | Record<string, string>
        | [string, string][],
    ): URLSearchParams;
  };

  declare function fetch(
    input: string | URL,
    init?: RequestInit,
  ): Promise<Response>;

  interface Response {
    ok: boolean;
    status: number;
    statusText: string;
    json(): Promise<any>;
    text(): Promise<string>;
  }

  interface URL {
    href: string;
    protocol: string;
    host: string;
    hostname: string;
    port: string;
    pathname: string;
    search: string;
    hash: string;
  }

  declare var URL: {
    prototype: URL;
    new (url: string): URL;
  };

  type TimerHandler = string | Function;

  declare function setTimeout(
    handler: TimerHandler,
    timeout?: number,
    ...arguments: any[]
  ): number;
  declare function clearTimeout(id?: number): void;
  declare function setInterval(
    handler: TimerHandler,
    timeout?: number,
    ...arguments: any[]
  ): number;
  declare function clearInterval(id?: number): void;

  declare function confirm(message: string): boolean;

  interface CSSStyleDeclaration {
    backgroundColor: string;
  }

  declare function getComputedStyle(
    elt: Element,
    pseudoElt?: string | null,
  ): CSSStyleDeclaration;
}

export {};
