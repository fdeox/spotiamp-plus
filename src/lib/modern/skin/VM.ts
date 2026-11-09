// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import { interpret } from "../maki/interpreter";
import { ParsedMaki } from "../maki/parser";
import { Variable } from "../maki/v";
import { UIRoot } from "../UIRoot";
import BaseObject from "./makiClasses/BaseObject";

import { classResolver } from "./resolver";

export default class Vm {
  _uiRoot: UIRoot; // actually only new Klass(uiRoot)
  _scripts: ParsedMaki[] = [];

  constructor(uiRoot: UIRoot) {
    this._uiRoot = uiRoot;
  }

  // This could easily become performance sensitive. We could make this more
  // performant by normalizing some of these things when scripts are added.
  dispatch1(object: BaseObject, event: string, args: Variable[] = []): number {
    const reversedArgs = [...args].reverse();
    let executed = 0;
    for (const script of this._scripts) {
      for (const binding of script.bindings) {
        if (
          script.methods[binding.methodOffset].name === event &&
          (script.variables[binding.variableOffset].value === object ||
            (script.variables[binding.variableOffset].isClass &&
              script.variables[binding.variableOffset].members.find(
                (vari) => script.variables[vari].value == object
              )))
        ) {
          if (event.startsWith("onleftbu")) {
            // debugger;
            console.log("EXEC EVENT:", event, binding);
          }
          this.interpret(script, binding.commandOffset, event, reversedArgs);
          executed++;
          // return 1;
        }
      }
    }
    if (event.startsWith("onleft")) {
      console.log("dispatched", executed, "x :", event, object._id);
    }
    return 0;
  }

  // This could easily become performance sensitive. We could make this more
  // performant by normalizing some of these things when scripts are added.
  // Spotiamp+: how deep events are set off from events now
  _dispatchDepth: number = 0;
  // Spotiamp+: events since the page last had a turn
  _burst: number = 0;
  _burstTurn: boolean = false;
  _burstWarned: boolean = false;

  dispatch(
    object: BaseObject,
    event: string,
    args: Variable[] = [],
    out?: { value?: any }
  ): Promise<number> {
    // Spotiamp+: a script whose event sets itself off again and again is
    // stopped there, not left to run the page out of stack (the events
    // running inside one another now, not all the ones still to finish)
    if (this._dispatchDepth > 200) return Promise.resolve(0);
    // and events set off from events round and round, the page never
    // getting a turn (scripts answering each other), cut off
    if (++this._burst > 20000) {
      if (!this._burstWarned) console.warn(`events going round and round: ${event} and the rest left out`);
      this._burstWarned = true;
      return Promise.resolve(0);
    }
    if (!this._burstTurn) {
      this._burstTurn = true;
      const channel = new MessageChannel();
      channel.port1.onmessage = () => {
        this._burst = 0;
        this._burstTurn = false;
      };
      channel.port2.postMessage(0);
    }
    this._dispatchDepth++;
    try {
      return this._dispatch(object, event, args, out);
    } finally {
      this._dispatchDepth--;
    }
  }

  // Spotiamp+: an event a skin's script answers with a value (Layer's
  // fx_onGetPixel*): what the script returned, undefined when none took it
  async dispatchValue(
    object: BaseObject,
    event: string,
    args: Variable[] = []
  ): Promise<any> {
    const out: { value?: any } = {};
    const executed = await this.dispatch(object, event, args, out);
    return executed ? out.value : undefined;
  }

  // Spotiamp+: whether any script takes this event for this object
  listens(object: BaseObject, event: string): boolean {
    for (const script of this._scripts) {
      for (const binding of script.bindings) {
        if (script.methods[binding.methodOffset].name !== event) continue;
        const v = script.variables[binding.variableOffset];
        if (v.isClass) {
          if (v.members.some((m) => script.variables[m].value == object)) return true;
        } else if (v.type === "OBJECT" && v.value === object) {
          return true;
        }
      }
    }
    return false;
  }

  async _dispatch(
    object: BaseObject,
    event: string,
    args: Variable[] = [],
    out?: { value?: any }
  ): number {
    const reversedArgs = [...args].reverse();
    let executed = 0;
    for (const script of this._scripts) {
      for (const binding of script.bindings) {
        if (script.methods[binding.methodOffset].name === event) {
          let match = false;
          // if(event.startsWith('onleftbu')){
          //   debugger;
          // }
          const binding_var = script.variables[binding.variableOffset];
          // if (binding_var.type === 'CLASS') {
          if (binding_var.isClass) {
            // const rootClass = classResolver(binding_var.guid);
            // if (object instanceof rootClass) {
            const found = binding_var.members.find((var_index) => {
              const member_var = script.variables[var_index];
              return member_var.value == object;
            });
            if (found != null) {
              // console.log('doEvent:', event, 'CLASS:', binding_var.guid, '@')
              binding_var.value = object;
              match = true;
            }
            // }
            // this.interpret(script, binding.commandOffset, reversedArgs);
            // // return 1;
            // executed ++;
          } else if (
            binding_var.type === "OBJECT" &&
            binding_var.value === object
          ) {
            match = true;
          }

          if (match) {
            // (Spotiamp+: each script that takes the event gets the
            // arguments, not what the one before it left over; what's left
            // on top when it's done is what it returned)
            const stack = [...reversedArgs];
            await this.interpret(script, binding.commandOffset, event, stack);
            if (out && stack.length) out.value = stack[stack.length - 1]?.value;
            // return 1;
            executed++;
          }
        }
      }
    }
    // if(event.startsWith('onleft')){
    //   console.log('dispatched',executed,'x :', event, object._id)
    // }
    return executed;
    // return 0;
  }

  addScript(maki: ParsedMaki): number {
    const index = this._scripts.length;
    this._scripts.push(maki);
    return index;
  }

  async interpret(
    script: ParsedMaki,
    commandOffset: number,
    eventName: string,
    args: Variable[]
  ) {
    // try{
    await interpret(
      commandOffset,
      script,
      args,
      classResolver,
      eventName,
      this._uiRoot
    );
    // } catch {
    //   console.log('ERRORdah')
    // }
  }
}
