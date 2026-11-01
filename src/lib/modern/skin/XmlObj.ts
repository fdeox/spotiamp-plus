// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import BaseObject from "./makiClasses/BaseObject";

export default class XmlObj extends BaseObject {
  setXmlAttributes(attributes: { [attrName: string]: string }) {
    for (const [key, value] of Object.entries(attributes)) {
      this.setXmlAttr(key, value);
    }
  }

  setXmlAttr(_key: string, _value: string): boolean {
    return false;
  }

  setxmlparam(key: string, value: string) {
    this.setXmlAttr(key, value);
  }
}
